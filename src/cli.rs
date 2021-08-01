use clap::{App, Arg};
use std::io;

#[derive(Debug, Clone)]
pub struct CliOptions {
    aws_function_name: String,
    discord_token: String,
}

pub fn parse_cli_options() -> io::Result<CliOptions> {
    let app = App::new("Tiltak playtak client")
        .version("0.1")
        .author("Morten Lohne")
        .arg(
            Arg::with_name("logfile")
                .short("l")
                .long("logfile")
                .value_name("tiltak.log")
                .help("Name of debug logfile")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("aws-function-name")
                .long("aws-function-name")
                .required(true)
                .help("Name of the aws function")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("discord-token")
                .long("discord-token")
                .help("Discord login token")
                .required(true)
                .takes_value(true),
        );
    let matches = app.get_matches();

    let log_dispatcher = fern::Dispatch::new().format(|out, message, record| {
        out.finish(format_args!(
            "{}[{}][{}] {}",
            chrono::Local::now().format("[%Y-%m-%d][%H:%M:%S]"),
            record.target(),
            record.level(),
            message
        ))
    });

    if let Some(log_file) = matches.value_of("logfile") {
        log_dispatcher
            .chain(
                fern::Dispatch::new()
                    .level(log::LevelFilter::Debug)
                    .chain(fern::log_file(log_file)?),
            )
            .chain(
                fern::Dispatch::new()
                    .level(log::LevelFilter::Warn)
                    .chain(io::stderr()),
            )
            .apply()
            .unwrap()
    } else {
        log_dispatcher
            .level(log::LevelFilter::Warn)
            .chain(io::stderr())
            .apply()
            .unwrap()
    }

    Ok(CliOptions {
        aws_function_name: matches.value_of("aws-function-name").unwrap().to_string(),
        discord_token: matches.value_of("discord-token").unwrap().to_string(),
    })
}
