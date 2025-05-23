use crate::mangalib::MangaPreview;
use crate::{config, mangalib, rabbitmq_consumer, send_resource, server};
use clap::{ArgMatches, Command, arg};
use futures::StreamExt;
use thiserror::Error;
use tokio::fs::File;
use tokio::io::AsyncWriteExt;

#[allow(clippy::cognitive_complexity)]
fn get_settings() -> Command {
    Command::new("mangalib")
        .about("Mangalib parser")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .allow_external_subcommands(true)
        .version(env!("CARGO_PKG_VERSION"))
        .subcommands([
            Command::new("serve")
                .about("Start web server")
                .arg(arg!(--port <PORT> "Web server port"))
                .arg(arg!(--browsers <BROWSERS> "Max chrome browsers count")),
            Command::new("send-resource")
                .about("Send start static resource")
                .arg(arg!(--url <URL> "URL where we should send this resource"))
                .arg_required_else_help(true),
            Command::new("collect-resource-full").about("Collect current resource to json"),
            Command::new("consume")
                .about("Consume RabbitMQ queue")
                .arg(arg!(--url <URL> "AMQP URI"))
                .arg(arg!(--browsers <BROWSERS> "Max chrome browsers count"))
                .arg_required_else_help(true),
        ])
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("No such command {0}")]
    NoSuchCommand(String),
    #[error("No command specified")]
    NoCommandSpecified,
    #[error("Web server error: {0}")]
    Serve(#[from] server::Error),
    #[error("Failed to send resources {0}")]
    SendResource(#[from] send_resource::Error),
    #[error("Failed to consume rabbitmq queue {0}")]
    Consume(#[from] rabbitmq_consumer::Error),
    #[error("Failed to parse arguments: {0}")]
    BadArgument(String),
}

pub async fn process_commands() -> Result<(), Error> {
    match get_settings().get_matches().subcommand() {
        Some(("serve", sub_matches)) => {
            let port = parse_port(sub_matches)?;
            let chrome_max_count = parse_chrome_max_count(sub_matches)?;

            serve(port, chrome_max_count).await
        }
        Some(("send-resource", sub_matches)) => {
            let url = sub_matches.get_one::<String>("url").expect("required");
            send_resource(url).await
        }
        Some(("consume", sub_matches)) => {
            let url = sub_matches.get_one::<String>("url").expect("required");
            let chrome_max_count = parse_chrome_max_count(sub_matches)?;

            consume(url, chrome_max_count).await
        }
        Some(("collect-resource-full", _sub_matches)) => {
            let iter = mangalib::search::get_manga_iter();
            let output = iter.collect::<Vec<MangaPreview>>().await;
            let mut file = File::create("resource/json/mangalib_manga_list.json")
                .await
                .expect("Cannot create resource/json/mangalib_manga_list.json file");
            file.write_all(
                serde_json::to_string(&output)
                    .expect("Cannot serialize output to json")
                    .as_bytes(),
            )
            .await
            .expect("Cannot write to file");

            Ok(())
        }
        Some((command, _)) => Err(Error::NoSuchCommand(command.to_string())),
        None => Err(Error::NoCommandSpecified),
    }
}

fn parse_chrome_max_count(sub_matches: &ArgMatches) -> Result<u16, Error> {
    sub_matches
        .get_one::<String>("browsers")
        .unwrap_or(&config::DEFAULT_CHROME_MAX_COUNT.to_string())
        .parse::<u16>()
        .map_err(|err| Error::BadArgument(format!("Failed to parse chrome max count: {err}")))
}

fn parse_port(sub_matches: &ArgMatches) -> Result<u16, Error> {
    sub_matches
        .get_one::<String>("port")
        .unwrap_or(&config::DEFAULT_APP_PORT.to_string())
        .parse::<u16>()
        .map_err(|err| Error::BadArgument(format!("Failed to parse port: {err}")))
}

async fn serve(port: u16, chrome_max_count: u16) -> Result<(), Error> {
    Ok(server::serve(port, chrome_max_count).await?)
}

async fn send_resource(url: &str) -> Result<(), Error> {
    Ok(send_resource::send_resource(url).await?)
}

async fn consume(url: &str, chrome_max_count: u16) -> Result<(), Error> {
    Ok(rabbitmq_consumer::consume(url, chrome_max_count).await?)
}
