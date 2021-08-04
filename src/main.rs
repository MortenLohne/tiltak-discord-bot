mod cli;

use pgn_traits::PgnPosition;
use serenity::async_trait;
use serenity::client::{Client, Context, EventHandler};
use serenity::framework::standard::{
    macros::{command, group},
    CommandResult, StandardFramework,
};
use serenity::model::channel::Message;
use std::error;
use tiltak::position::{Move, Position};

#[group]
#[commands(analyze_tps, ping)]
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

fn analyze_tps_sized<const S: usize>(tps: &str) -> Result<(f32, Vec<Move>), Box<dyn error::Error>> {
    let position: Position<S> = Position::from_fen(tps).unwrap();
    let mut tree = tiltak::search::MonteCarloTree::new(position);
    for _ in 0..100_000 {
        tree.select();
    }
    Ok((tree.mean_action_value(), tree.pv().collect()))
}
