use clap::ArgMatches;
use tdl::api::auth::AuthClient;
use tdl::cli::{cli, parse_config_flags};
use tdl::config::CONFIG;
use tdl::download::dispatch_downloads;
use tdl::download::ReceiveChannel;
use tdl::login::*;

use env_logger::Env;
use futures::future::join_all;
use futures::StreamExt;

use log::debug;
use tokio::join;
use tokio_stream::wrappers::ReceiverStream;

#[tokio::main]
async fn main() {
    // read from config to always trigger initialization of the default config if it doesn't exist
    // then release lock immediately.
    {
        let _ = CONFIG.read().await;
    }
    env_logger::Builder::from_env(Env::default().default_filter_or("none")).init();
    let matches = cli().get_matches();
    match matches.subcommand() {
        Some(("get", get_matches)) => get(get_matches).await,
        Some(("login", _)) => {
            login().await;
        }
        Some(("logout", _)) => logout().await,
        _ => unreachable!(), // If all subcommands are defined above, anything else is unreachable!()
    }
}

async fn get(matches: &ArgMatches) {
    let client = login().await;

    parse_config_flags(matches).await;
    if let Some(urls) = matches.get_many::<String>("URL") {
        let url: Vec<String> = urls.map(|i| i.to_owned()).collect();
        debug!("Collected args");
        let (handles, download, worker) = dispatch_downloads(url, client)
            .await
            .expect("Unable to dispatch download thread");
        let config = CONFIG.read().await;
        join!(
            join_all(handles),
            consume_channel(download, config.downloads.into(),),
            consume_channel(worker, config.workers.into())
        );
    }
}

async fn consume_channel(channel: ReceiveChannel, concurrency: usize) {
    //The channel receives an unexecuted future as a stream
    ReceiverStream::new(channel)
        //execute that future in a greenthread
        .map(|i| async { tokio::task::spawn(i).await })
        //up to a maximum concurrent tasks at a single time
        .buffer_unordered(concurrency)
        .for_each(|r| async {
            match r {
                Ok(l) => match l {
                    Ok(_) => {}
                    //if the task failed
                    Err(f) => eprintln!("{f}"),
                },
                // if we failed to launch the task
                Err(e) => eprintln!("{e}"),
            }
        })
        .await;
}

async fn logout() {
    let config = CONFIG.read().await;
    match config.login_key.access_token.clone() {
        Some(token) => match AuthClient::new(config.api_key.clone())
            .logout(token.to_owned())
            .await
        {
            Ok(_) => println!("Logout Sucessful"),
            Err(e) => eprintln!("Error Logging out: {e}"),
        },
        None => println!("No Auth Token is configured to logout with"),
    }
}
