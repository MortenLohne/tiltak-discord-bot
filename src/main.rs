mod aws;
mod cli;

use crate::aws::Output;
use board_game_traits::Position as PositionTrait;
use pgn_traits::PgnPosition;
use serenity::async_trait;
use serenity::client::{Client, Context, EventHandler};
use serenity::framework::standard::{
    macros::{command, group},
    CommandResult, StandardFramework,
};
use serenity::model::channel::Message;
use std::error::Error;
use tiltak::position::{Move, Position};

#[group]
#[commands(analyze_ptn, analyze_tps, ping, analyze_startpos)]
struct General;

struct Handler;

#[async_trait]
impl EventHandler for Handler {}

#[tokio::main]
async fn main() {
    let cli_options = cli::parse_cli_options().unwrap();
    println!("Options: {:?}", cli_options);

    let framework = StandardFramework::new()
        .configure(|c| c.prefix("~")) // set the bot's prefix to "~"
        .group(&GENERAL_GROUP);

    println!("Initialized framework");

    // Login with a bot token from the environment
    let mut client = Client::builder(cli_options.discord_token)
        .event_handler(Handler)
        .framework(framework)
        .await
        .expect("Error creating client");

    println!("Logged in");

    // start listening for events by starting a single shard
    if let Err(why) = client.start().await {
        println!("An error occurred while running the client: {:?}", why);
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
    if let Some((_, ptn_text)) = msg.content.split_once(|ch: char| ch.is_whitespace()) {
        for line in ptn_text.lines() {
            println!("{}, {}", line, line.contains("Size"));
        }
        if let Some(size_line) = ptn_text.lines().find(|line| line.contains("Size")) {
            let analysis = match size_line
                .split_whitespace()
                .nth(1)
                .and_then(|r| r.chars().nth(1).and_then(|r| r.to_digit(10)))
            {
                Some(4) => analyze_ptn_sized::<4>(ctx, msg, ptn_text).await?,
                Some(5) => analyze_ptn_sized::<5>(ctx, msg, ptn_text).await?,
                Some(6) => analyze_ptn_sized::<6>(ctx, msg, ptn_text).await?,
                Some(s) => {
                    msg.reply(ctx, format!("Size {} is unsupported", s)).await?;
                    return Ok(());
                }
                None => {
                    msg.reply(ctx, "Couldn't determine size for ptn").await?;
                    return Ok(());
                }
            };

            msg.reply(ctx, format!("{:?}", analysis)).await?;
            Ok(())
        } else {
            msg.reply(ctx, "Couldn't determine size for ptn").await?;
            Ok(())
        }
    } else {
        msg.reply(ctx, "No PTN provided").await?;
        Err("No PTN provided".into())
    }
}

#[command]
async fn analyze_startpos(ctx: &Context, msg: &Message) -> CommandResult {
    println!("Received {} from {}", msg.content, msg.author.name);
    let future = aws::pv_aws("Taik", 6, vec![], 100_000);

    let aws::Output { pv, score } = future.await.unwrap();
    msg.reply(
        ctx,
        format!("{:.1}%: {}", score * 100.0, pv[0].to_string::<6>()),
    )
    .await?;

    Ok(())
}

#[command]
async fn analyze_tps(ctx: &Context, msg: &Message) -> CommandResult {
    let (_, tps) = msg
        .content
        .split_once(|ch: char| ch.is_whitespace())
        .unwrap();
    let size = tps.chars().filter(|ch| *ch == '/').count() + 1;
    println!(
        "Received {} with tps {} size {} from {}",
        msg.content, tps, size, msg.author.name
    );
    let (eval, pv) = match size {
        4 => analyze_tps_sized::<4>(tps).unwrap(),
        5 => analyze_tps_sized::<5>(tps).unwrap(),
        6 => analyze_tps_sized::<6>(tps).unwrap(),
        _ => unimplemented!(),
    };
    msg.reply(
        ctx,
        format!("{:.1}%: {}", eval * 100.0, pv[0].to_string::<5>()),
    )
    .await?;
    Err("error".into())
}

fn analyze_tps_sized<const S: usize>(tps: &str) -> CommandResult<(f32, Vec<Move>)> {
    let position: Position<S> = Position::from_fen(tps).unwrap();
    let mut tree = tiltak::search::MonteCarloTree::new(position);
    for _ in 0..100_000 {
        tree.select();
    }
    Ok((tree.mean_action_value(), tree.pv().collect()))
}

#[derive(Clone, Debug, PartialOrd, PartialEq)]
struct GameAnalysis {
    game_tags: Vec<(String, String)>,
    move_strings: Vec<String>,
    move_scores: Vec<f32>,
    pvs: Vec<String>,
}

async fn analyze_ptn_sized<const S: usize>(
    ctx: &Context,
    msg: &Message,
    ptn: &str,
) -> Result<GameAnalysis, Box<dyn Error + Send + Sync>> {
    match tiltak::ptn::ptn_parser::parse_ptn::<Position<S>>(ptn) {
        Ok(games) => {
            if games.is_empty() {
                msg.reply(ctx, "Error: parsed 0 games").await?;
                return Err("Error: parsed 0 games".into());
            }
            let game = &games[0];
            if game.start_position != Position::start_position() {
                msg.reply(ctx, "Cannot analyze games with a custom start position")
                    .await?;
                return Err("Cannot analyze games with a custom start position".into());
            }

            let futures = (0..=game.moves.len()).map(|i| {
                let moves = game.moves[0..i]
                    .iter()
                    .map(|ptn_move| ptn_move.mv.clone())
                    .collect();
                aws::pv_aws("Taik", S, moves, 100_000)
            });
            // Some trickery to transform Vec<Result<_>> into Result<Vec<_>>
            let results = futures::future::join_all(futures).await;
            let result_results: Result<Vec<_>, _> = results.into_iter().collect();
            match result_results {
                Err(_) => {
                    msg.reply(ctx, "AWS error").await?;
                    Err("AWS error".into())
                }
                Ok(outputs) => {
                    let move_annotations = annotate_move_scores(
                        &outputs
                            .iter()
                            .map(|output| output.score)
                            .collect::<Vec<f32>>(),
                    );
                    let game_move_strings = game
                        .moves
                        .iter()
                        .zip(move_annotations)
                        .map(|(mv, annotation)| mv.mv.to_string::<S>() + annotation)
                        .collect();
                    let (move_scores, pvs) = outputs
                        .into_iter()
                        .map(|Output { score, pv }| {
                            let pv_strings: Vec<String> =
                                pv.iter().take(3).map(|mv| mv.to_string::<S>()).collect();
                            (score, pv_strings.join(" "))
                        })
                        .unzip();

                    Ok(GameAnalysis {
                        game_tags: game.tags.clone(),
                        move_strings: game_move_strings,
                        move_scores,
                        pvs,
                    })
                }
            }
        }
        Err(err) => {
            msg.reply(ctx, err.to_string()).await?;
            Err(err)
        }
    }
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
