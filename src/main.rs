mod aws;
mod cli;

use crate::aws::Output;
use board_game_traits::Position as PositionTrait;
use log::warn;
use once_cell::sync::OnceCell;
use serenity::async_trait;
use serenity::client::{Client, Context, EventHandler};
use serenity::framework::standard::{
    macros::{command, group},
    CommandResult, StandardFramework,
};
use serenity::http::Typing;
use serenity::model::channel::Message;
use serenity::model::prelude::AttachmentType;
use serenity::prelude::GatewayIntents;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time;
use tiltak::position::{Komi, Position};
use tiltak::ptn::{Game, PtnMove};

static AWS_FUNCTION_NAME: OnceCell<String> = OnceCell::new();

static CURRENTLY_ANALYZING: AtomicBool = AtomicBool::new(false);

static GAMES_ANALYZED: AtomicUsize = AtomicUsize::new(0);
const MAX_GAMES_ANALYZED: usize = 200;

#[group]
#[commands(analyze_ptn, analyze_tps, ping)]
struct General;

struct Handler;

#[async_trait]
impl EventHandler for Handler {}

#[tokio::main]
async fn main() {
    let cli_options = cli::parse_cli_options().unwrap();
    println!("Options: {cli_options:?}");

    AWS_FUNCTION_NAME
        .set(cli_options.aws_function_name)
        .unwrap();

    let framework = StandardFramework::new()
        .configure(|c| c.prefix("!")) // set the bot's prefix to "~"
        .group(&GENERAL_GROUP);

    println!("Initialized framework");

    // Login with a bot token from the environment
    let mut client = Client::builder(
        cli_options.discord_token,
        GatewayIntents::non_privileged().union(GatewayIntents::MESSAGE_CONTENT),
    )
    .event_handler(Handler)
    .framework(framework)
    .await
    .expect("Error creating client");

    println!("Logged in");

    // start listening for events by starting a single shard
    if let Err(why) = client.start().await {
        println!("An error occurred while running the client: {why:?}");
    }
}

#[command]
async fn ping(ctx: &Context, msg: &Message) -> CommandResult {
    println!("Received {} from {}", msg.content, msg.author.name);
    msg.reply(ctx, "Pong!").await?;

    Ok(())
}

#[command]
async fn analyze_ptn(ctx: &Context, msg: &Message) -> CommandResult {
    println!("Received {} from {}", msg.content, msg.author.name);
    if msg.guild_id.is_none() {
        msg.reply(ctx, "Analysis is only available in specific channels.")
            .await?;
        return Ok(());
    }
    if let Some((_, ptn_text)) = msg.content.split_once(|ch: char| ch.is_whitespace()) {
        if let Some(size_line) = ptn_text.lines().find(|line| line.contains("Size")) {
            match size_line
                .split_whitespace()
                .nth(1)
                .and_then(|r| r.chars().nth(1).and_then(|r| r.to_digit(10)))
            {
                Some(4) => analyze_ptn_sized::<4>(ctx, msg, ptn_text).await?,
                Some(5) => analyze_ptn_sized::<5>(ctx, msg, ptn_text).await?,
                Some(6) => analyze_ptn_sized::<6>(ctx, msg, ptn_text).await?,
                Some(s) => {
                    msg.reply(ctx, format!("Size {s} is unsupported.")).await?;
                    return Ok(());
                }
                None => {
                    msg.reply(
                        ctx,
                        "Invalid Size tag. The PTN must include a tag such as [Size \"6\"].",
                    )
                    .await?;
                    return Ok(());
                }
            };
            Ok(())
        } else {
            msg.reply(
                ctx,
                "Couldn't determine board size. The PTN must include a tag such as [Size \"6\"].",
            )
            .await?;
            Ok(())
        }
    } else {
        msg.reply(ctx, "No PTN provided.").await?;
        Ok(())
    }
}

#[command]
async fn analyze_tps(ctx: &Context, msg: &Message) -> CommandResult {
    if let Some((_, tps)) = msg.content.split_once(|ch: char| ch.is_whitespace()) {
        let size = tps.chars().filter(|ch| *ch == '/').count() + 1;
        println!(
            "Received {} with tps {} size {} from {}",
            msg.content, tps, size, msg.author.name
        );
        match size {
            0..=3 => msg.reply(ctx, "Couldn't read tps.").await?,
            4..=6 => msg.reply(ctx, "Not implemented yet!").await?,
            s => msg.reply(ctx, format!("Size {s} is unsupported.")).await?,
        };
        Ok(())
    } else {
        msg.reply(ctx, "Couldn't read tps.").await?;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialOrd, PartialEq)]
struct GameAnalysis {
    game_tags: Vec<(String, String)>,
    move_strings: Vec<String>,
    comments: Vec<String>,
}

async fn analyze_ptn_sized<const S: usize>(
    ctx: &Context,
    msg: &Message,
    ptn: &str,
) -> CommandResult {
    match tiltak::ptn::ptn_parser::parse_ptn::<Position<S>>(ptn) {
        Ok(games) => {
            if games.is_empty() {
                msg.reply(ctx, "Error: parsed 0 games.").await?;
                return Ok(());
            }
            let game = &games[0];
            if game.start_position != Position::start_position() {
                msg.reply(ctx, "Cannot analyze games with a custom start position.")
                    .await?;
                return Ok(());
            }

            if game.moves.len() > 200 {
                msg.reply(ctx, "Game length cannot exceed 100 moves.")
                    .await?;
                return Ok(());
            }

            let komi_string = game
                .tags
                .iter()
                .find_map(|(tag, value)| {
                    if tag == "Komi" {
                        Some(value.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| "0".to_string());

            let komi = match Komi::from_str(&komi_string) {
                Ok(komi) => komi,
                Err(_) => {
                    msg.reply(ctx, format!("Couldn't analyze with {komi_string} komi"))
                        .await?;
                    return Ok(());
                }
            };

            let eval_komi = match (S, komi.half_komi()) {
                (4, ..=3) => Komi::from_half_komi(0).unwrap(),
                (4, 4..) => Komi::from_half_komi(8).unwrap(),
                (5, ..=1) => Komi::from_half_komi(0).unwrap(),
                (5, 2..) => Komi::from_half_komi(4).unwrap(),
                (6, ..=1) => Komi::from_half_komi(0).unwrap(),
                (6, 2..) => Komi::from_half_komi(4).unwrap(),
                (_, _) => {
                    msg.reply(ctx, format!("Size {S} with komi {komi} is unsupported."))
                        .await?;
                    return Ok(());
                }
            };

            if komi != eval_komi {
                msg.reply(
                    ctx,
                    format!("Note: {komi} komi on {S}s is not fully supported. Until the endgame, the game will be evaluated as if it had {eval_komi} komi."),
                )
                .await?;
            }

            if GAMES_ANALYZED.load(Ordering::SeqCst) > MAX_GAMES_ANALYZED {
                msg.reply(ctx, "Too many games analyzed recently. Try again later.")
                    .await?;
                return Ok(());
            } else {
                GAMES_ANALYZED.fetch_add(1, Ordering::SeqCst);
            }

            if CURRENTLY_ANALYZING
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_err()
            {
                msg.reply(
                    ctx,
                    "Cannot analyze two games simultaneously. Try again later.",
                )
                .await?;
                return Ok(());
            }

            let typing = Typing::start(ctx.http.clone(), msg.channel_id.0)?;

            let start_time = time::Instant::now();

            let futures = (0..=game.moves.len()).map(|i| {
                let moves = game.moves[0..i]
                    .iter()
                    .map(|ptn_move| ptn_move.mv.to_string::<S>())
                    .collect();
                aws::pv_aws(S, moves, 500_000, komi, eval_komi)
            });
            let results = futures::future::join_all(futures).await;

            typing.stop().unwrap();
            CURRENTLY_ANALYZING.store(false, Ordering::SeqCst);

            // Some trickery to transform Vec<Result<_>> into Result<Vec<_>>
            let result_results: Result<Vec<_>, _> = results.into_iter().collect();
            match result_results {
                Err(error) => {
                    warn!("AWS error: {}", error);
                    msg.reply(ctx, "AWS error.").await?;
                    Err("AWS error".into())
                }
                Ok(outputs) => {
                    let (file_contents, white_name, black_name) = process_aws_output(game, outputs);
                    println!("{}", std::str::from_utf8(file_contents.as_slice()).unwrap());

                    let channel = msg.channel(&ctx).await.unwrap();

                    channel
                        .id()
                        .send_message(&ctx.http, |m| {
                            m.content(format!(
                                "Finished analyzing {} vs {} in {:.1}s. Best viewed in ptn.ninja!",
                                white_name,
                                black_name,
                                start_time.elapsed().as_secs_f32()
                            ));
                            m.add_file(AttachmentType::Bytes {
                                data: file_contents.into(),
                                filename: format!("{white_name}_vs_{black_name}.txt"),
                            });
                            m
                        })
                        .await?;
                    Ok(())
                }
            }
        }
        Err(err) => {
            msg.reply(ctx, err.to_string()).await?;
            Err(err)
        }
    }
}

fn process_aws_output<const S: usize>(
    game: &Game<Position<S>>,
    outputs: Vec<Output>,
) -> (Vec<u8>, String, String) {
    let (move_scores, pv_strings): (Vec<f32>, Vec<Vec<String>>) = outputs
        .into_iter()
        .map(|Output { score, pv, .. }| (score, pv))
        .unzip();
    let move_annotations = annotate_move_scores(&move_scores);

    let comments = move_scores
        .iter()
        .skip(1)
        .zip(pv_strings)
        .map(|(score, pv)| {
            let pv_moves = pv.iter().take(3).map(String::as_str).collect::<Vec<&str>>();
            format!("{:.1}%, pv {}", score * 100.0, pv_moves.join(" "))
        });

    let annotated_game = Game {
        start_position: game.start_position.clone(),
        moves: game
            .moves
            .iter()
            .zip(move_annotations.into_iter())
            .zip(comments)
            .map(|((ptn_move, annotation), comment)| PtnMove {
                mv: ptn_move.mv.clone(),
                annotations: if annotation.is_empty() {
                    vec![]
                } else {
                    vec![annotation]
                },
                comment,
            })
            .collect(),
        game_result_str: game.game_result_str,
        tags: game.tags.clone(),
    };

    let white_name = annotated_game
        .tags
        .iter()
        .find_map(|(tag, value)| {
            if tag == "Player1" {
                Some(value.clone())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "?".to_string());

    let black_name = annotated_game
        .tags
        .iter()
        .find_map(|(tag, value)| {
            if tag == "Player2" {
                Some(value.clone())
            } else {
                None
            }
        })
        .unwrap_or_else(|| "?".to_string());

    let mut buffer = Vec::new();
    annotated_game.game_to_ptn(&mut buffer).unwrap();
    (buffer, white_name, black_name)
}

fn annotate_move_scores(move_scores: &[f32]) -> Vec<&'static str> {
    move_scores
        .windows(2)
        .enumerate()
        .map(|(i, scores)| {
            let last_score = scores[0];
            let score = scores[1];

            let score_loss = if i % 2 == 0 {
                // The current move was made by white
                score - last_score
            } else {
                last_score - score
            };

            if score_loss > 0.06 {
                "!!"
            } else if score_loss > 0.03 {
                "!"
            } else if score_loss > -0.1 {
                ""
            } else if score_loss > -0.25 {
                "?"
            } else {
                "??"
            }
        })
        .collect()
}
