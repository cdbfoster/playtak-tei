use std::env;
use std::io;
use std::time::Duration;

use async_std::io::{BufReader, WriteExt};
use async_std::net::TcpStream;
use async_std::prelude::*;
use async_std::process::{Command, Stdio};
use async_std::task;
use clap::{arg, command, Args, Parser};
use futures::{select, AsyncWrite, FutureExt};
use tracing::{debug, error, info};

use self::game::{Game, GameMove};
use self::option::{validate_and_set_option, SpinOption};
use self::seek::{capstones_for_size, flatstones_for_size, Seek};

mod game;
mod option;
mod seek;

#[derive(Args, Clone, Debug)]
struct Login {
    #[arg(short = 't', long = "token", group = "login")]
    guest_token: Option<String>,
    #[arg(short, long, group = "login", requires = "password")]
    username: Option<String>,
    #[arg(short, long, requires = "username")]
    password: Option<String>,
}

impl Login {
    fn to_login_string(&self) -> String {
        format!(
            "Login {}\n",
            if let (Some(username), Some(password)) = (&self.username, &self.password) {
                format!("{username} {password}")
            } else {
                format!(
                    "Guest{}",
                    if let Some(token) = &self.guest_token {
                        format!(" {token}")
                    } else {
                        String::new()
                    }
                )
            }
        )
    }
}

#[derive(Args, Debug)]
struct ListCommand {
    #[command(flatten)]
    login: Login,
}

#[derive(Args, Debug)]
#[group(required = true)]
struct AcceptInfo {
    #[arg(short, long = "seek")]
    seek_id: Option<u32>,
    #[arg(short, long)]
    opponent: Option<String>,
}

#[derive(Args, Debug)]
struct AcceptCommand {
    #[command(flatten)]
    login: Login,
    #[command(flatten)]
    accept: AcceptInfo,
    #[arg(required = true, num_args = 1.., trailing_var_arg = true)]
    engine_arguments: Vec<String>,
}

#[derive(Args, Debug)]
struct SeekCommand {
    #[command(flatten)]
    login: Login,
    #[command(flatten)]
    seek: Seek,
    #[arg(required = true, num_args = 1.., trailing_var_arg = true)]
    engine_arguments: Vec<String>,
}

#[derive(Debug, Parser)]
enum ArgCommand {
    List(ListCommand),
    Accept(AcceptCommand),
    Seek(SeekCommand),
}

fn main() {
    let args = ArgCommand::parse();

    tracing_subscriber::fmt::init();

    // Limit the number of threads async-std tries to spawn; we don't need that many.
    if env::var("ASYNC_STD_THREAD_COUNT").is_err() {
        env::set_var("ASYNC_STD_THREAD_COUNT", "1");
    }

    task::block_on(main_inner(args)).ok();
}

macro_rules! assert_response {
    ($reader:expr, $value:expr) => {
        let line = read($reader).await?;
        if line != $value {
            error!(received = ?line, expected = $value, "Unexpected value.");
            return Err(io::Error::from(io::ErrorKind::InvalidData));
        }
    };
}

macro_rules! err {
    () => {
        io::Error::from(io::ErrorKind::Other)
    };
    ($message:expr) => {
        io::Error::new(io::ErrorKind::Other, $message)
    };
}
pub(crate) use err;

trait Writer: AsyncWrite + WriteExt + Unpin {}
impl<T> Writer for T where T: AsyncWrite + WriteExt + Unpin {}

async fn write(mut writer: impl Writer, value: impl AsRef<[u8]>) -> io::Result<()> {
    if let Ok(value) = std::str::from_utf8(value.as_ref()) {
        debug!(?value, "Sending");
    }

    let result = writer.write_all(value.as_ref()).await;

    if let Err(error) = &result {
        error!(%error, "Could not write to stream.");
    }

    result
}

trait Reader: Stream<Item = io::Result<String>> + Unpin {}
impl<T> Reader for T where T: Stream<Item = io::Result<String>> + Unpin {}

async fn read(mut reader: impl Reader) -> io::Result<String> {
    let result = if let Some(next) = reader.next().await {
        next
    } else {
        error!("Stream closed unexpectedly.");
        return Err(io::Error::from(io::ErrorKind::UnexpectedEof));
    };

    if let Err(error) = &result {
        error!(%error, "Could not read from stream.");
    }

    if let Ok(line) = &result {
        debug!(?line, "Received");
    }

    result
}

async fn main_inner(args: ArgCommand) -> io::Result<()> {
    let (mut playtak_writer, mut playtak_reader) =
        match TcpStream::connect("playtak.com:10000").await {
            Ok(stream) => {
                info!("Connected to PlayTak.com.");
                (stream.clone(), BufReader::new(stream).lines().fuse())
            }
            Err(error) => {
                error!(%error, "Could not connect to PlayTak.com.");
                return Err(error);
            }
        };

    assert_response!(&mut playtak_reader, "Welcome!");
    assert_response!(&mut playtak_reader, "Login or Register");

    write(&mut playtak_writer, "Client playtak-tei\n").await?;

    assert_response!(&mut playtak_reader, "OK");

    debug!("Client acknowledged.");

    let login_name = match &args {
        ArgCommand::List(ListCommand { login })
        | ArgCommand::Accept(AcceptCommand { login, .. })
        | ArgCommand::Seek(SeekCommand { login, .. }) => {
            write(&mut playtak_writer, login.to_login_string()).await?;

            let response = read(&mut playtak_reader).await?;
            if response == "Authentication failure" {
                error!("Could not authenticate. Are the username and password correct?");
                return Err(err!());
            } else if response.starts_with("Game Start") {
                info!("Resuming game.");

                let mut game = response.parse::<Game>().map_err(|error| err!(error))?;

                'resume: loop {
                    let line = read(&mut playtak_reader).await?;

                    if line != "Message Your game is resumed" {
                        let parts = line.split_ascii_whitespace().collect::<Vec<_>>();

                        if parts[1] == "P" || parts[1] == "M" {
                            game.moves.push(GameMove::from_playtak(&line)?);
                        } else if parts[1] == "Time" {
                            game.time = (
                                parts[2]
                                    .parse::<u32>()
                                    .map_err(|_| err!("could not parse white time"))?,
                                parts[3]
                                    .parse::<u32>()
                                    .map_err(|_| err!("could not parse black time"))?,
                            );
                        }
                    } else {
                        break 'resume;
                    }
                }

                let (engine_writer, engine_reader) = initialize_engine(&args, &game).await?;

                return run_game(
                    game,
                    (engine_writer, engine_reader),
                    (playtak_writer, playtak_reader),
                )
                .await;
            } else if !response.starts_with("Welcome") {
                error!("Could not log in.");
                return Err(err!());
            } else {
                response
                    .split_ascii_whitespace()
                    .nth(1)
                    .and_then(|n| n.strip_suffix('!'))
                    .map(|n| n.to_owned())
                    .expect("could not parse login name")
            }
        }
    };

    info!("Logged in as {login_name}.");

    let mut seeks = Vec::new();
    loop {
        let input = read(&mut playtak_reader).await?;

        // Read only until the server is done sending seeks.
        if input.starts_with("Seek new") {
            seeks.push(input.parse::<Seek>().map_err(|error| err!(error))?);
        } else {
            break;
        }
    }

    if matches!(args, ArgCommand::List(_)) {
        println!("Available seeks:\n");

        for seek in seeks {
            println!("{seek}\n");
        }

        return write(&mut playtak_writer, "quit\n").await;
    }

    task::spawn(ping(playtak_writer.clone()));

    // Post or accept the seek.
    match &args {
        ArgCommand::Accept(AcceptCommand {
            accept: AcceptInfo { seek_id, opponent }, ..
        }) => {
            if let Some(seek_id) = seek_id {
                info!("Accepting seek {seek_id}.");
                write(&mut playtak_writer, format!("Accept {seek_id}\n")).await?;
            } else if let Some(opponent) = opponent {
                if let Some(seek) = seeks.iter().find(|s| s.player.as_ref() == Some(opponent)) {
                    let seek_id = seek.id.unwrap();
                    info!(id = seek_id, "Accepting seek from {opponent}.");
                    write(&mut playtak_writer, format!("Accept {seek_id}\n")).await?;
                } else {
                    error!("Cannot find seek from {opponent}.");
                    return Err(err!());
                }
            }
        }
        ArgCommand::Seek(SeekCommand { seek, .. }) => {
            info!("Posting seek.");
            write(&mut playtak_writer, seek.to_seek_string()).await?;
        }
        _ => unreachable!(),
    }

    let game = loop {
        let line = read(&mut playtak_reader).await?;

        if line == "NOK" {
            error!("Could not accept or post seek.");
            return Err(err!());
        } else if line.starts_with("Game Start") {
            break line.parse::<Game>().map_err(|error| err!(error))?;
        }
    };

    let (engine_writer, engine_reader) = initialize_engine(&args, &game).await?;

    run_game(
        game,
        (engine_writer, engine_reader),
        (playtak_writer, playtak_reader),
    )
    .await
}

async fn ping(mut writer: TcpStream) -> io::Result<()> {
    loop {
        task::sleep(Duration::from_secs(30)).await;
        write(&mut writer, "PING\n").await?;
    }
}

async fn initialize_engine(
    args: &ArgCommand,
    game: &Game,
) -> io::Result<(impl Writer, impl Reader)> {
    let (mut engine_writer, mut engine_reader) = {
        let (engine, arguments) = match &args {
            ArgCommand::Accept(AcceptCommand {
                engine_arguments, ..
            })
            | ArgCommand::Seek(SeekCommand {
                engine_arguments, ..
            }) => (engine_arguments[0].as_str(), &engine_arguments[1..]),
            _ => unreachable!(),
        };

        let mut child = Command::new(engine)
            .args(arguments)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;

        (
            child.stdin.take().unwrap(),
            BufReader::new(child.stdout.take().unwrap()).lines().fuse(),
        )
    };

    write(&mut engine_writer, "tei\n").await?;

    let mut engine_name = "TEI engine".to_owned();
    let mut engine_options = Vec::new();
    loop {
        let line = read(&mut engine_reader).await?;

        if line.starts_with("id name") {
            engine_name = line.strip_prefix("id name ").unwrap().to_owned();
        } else if line.starts_with("option") && line.contains("type spin") {
            engine_options.push(line.parse::<SpinOption>().map_err(|error| err!(error))?);
        } else if line == "teiok" {
            break;
        }
    }

    // Validate the game options with the available engine options and set them.

    validate_and_set_option(
        &mut engine_writer,
        &engine_options,
        "HalfKomi",
        game.half_komi as i32,
        0,
    )
    .await?;
    validate_and_set_option(
        &mut engine_writer,
        &engine_options,
        "Flatstones",
        game.flatstones as i32,
        flatstones_for_size(game.size) as i32,
    )
    .await?;
    validate_and_set_option(
        &mut engine_writer,
        &engine_options,
        "Capstones",
        game.capstones as i32,
        capstones_for_size(game.size) as i32,
    )
    .await?;

    info!("{engine_name} initialized.");

    Ok((engine_writer, engine_reader))
}

async fn run_game(
    mut game: Game,
    (mut engine_writer, mut engine_reader): (impl Writer, impl Reader),
    (mut playtak_writer, mut playtak_reader): (impl Writer, impl Reader),
) -> io::Result<()> {
    info!(
        id = game.id,
        size = game.size,
        opponent = game.opponent,
        color = game.color,
        "Starting game."
    );

    let our_turn = match (&game.color, game.moves.len()) {
        (c, n) if c == "white" && n % 2 == 0 => true,
        (c, n) if c == "black" && n % 2 == 1 => true,
        _ => false,
    };

    if our_turn {
        write(&mut engine_writer, game.new_game_string()).await?;
        write(&mut engine_writer, game.position_string()).await?;
        write(&mut engine_writer, game.search_string()).await?;
    }

    'game: loop {
        select! {
            line = read(&mut engine_reader).fuse() => {
                let line = line?;

                let parts = line.split_ascii_whitespace().collect::<Vec<_>>();

                if parts[0] == "bestmove" {
                    let game_move = GameMove::from_ptn(parts[1])?;

                    write(&mut playtak_writer, game_move.to_playtak(game.id)).await?;

                    game.moves.push(game_move);
                }
            }
            line = read(&mut playtak_reader).fuse() => {
                let line = line?;

                let parts = line.split_ascii_whitespace().collect::<Vec<_>>();

                if parts[0] == "NOK" {
                    error!("Received NOK from PlayTak.com");
                }

                if parts[0] != format!("Game#{}", game.id) {
                    continue;
                }

                if parts[1] == "Time" {
                    game.time = (
                        parts[2].parse::<u32>().map_err(|_| err!("could not parse white time"))?,
                        parts[3].parse::<u32>().map_err(|_| err!("could not parse black time"))?,
                    );
                } else if parts[1] == "P" || parts[1] == "M" {
                    let game_move = GameMove::from_playtak(&line)?;

                    game.moves.push(game_move);

                    write(&mut engine_writer, game.position_string()).await?;
                    write(&mut engine_writer, game.search_string()).await?;
                } else if parts[1] == "Over" {
                    info!(result = parts[2], "Game finished.");
                    break 'game;
                }
            }
        }
    }

    Ok(())
}
