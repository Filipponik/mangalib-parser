use clap::{arg, Command};
use crate::{send_resource, server};

pub fn get_settings() -> Command {
    Command::new("mangalib")
        .about("Mangalib parser")
        .subcommand_required(true)
        .arg_required_else_help(true)
        .allow_external_subcommands(true)
        .subcommands([
            Command::new("serve")
                .about("Start web server"),
            Command::new("send-resource")
                .about("Send start static resource")
                .arg(arg!(--url <URL> "URL where we should send this resource"))
                .arg_required_else_help(true),
        ])
}

pub async fn process_commands() {
    match get_settings().get_matches().subcommand() {
        Some(("serve", _)) => command_serve().await,
        Some(("send-resource", sub_matches)) => {
            let url = sub_matches.get_one::<String>("url").expect("required");
            send_resource(url).await
        },
        Some(_) | None => panic!("No command specified"),
    }
}

pub async fn command_serve() {
    server::serve().await
}

pub async fn send_resource(url: &str) {
    send_resource::send_resource(url).await;
}