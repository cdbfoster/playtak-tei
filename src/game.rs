use std::fmt::Write;
use std::io;
use std::str::FromStr;

use super::err;

#[derive(Debug, Default)]
pub struct Game {
    pub id: u32,
    pub size: u32,
    pub opponent: String,
    pub color: String,
    pub time: (u32, u32),
    pub half_komi: u32,
    pub flatstones: u32,
    pub capstones: u32,
    pub moves: Vec<GameMove>,
}

impl FromStr for Game {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts = s.split_ascii_whitespace().collect::<Vec<_>>();

        let color = match parts[7] {
            value if value == "white" || value == "black" => value.to_owned(),
            _ => return Err("could not parse player color"),
        };

        let opponent = match color.as_str() {
            "white" => parts[6],
            "black" => parts[4],
            _ => unreachable!(),
        }
        .to_owned();

        let time = parts[8]
            .parse::<u32>()
            .map_err(|_| "could not parse game time")?;

        Ok(Self {
            id: parts[2]
                .parse::<u32>()
                .map_err(|_| "could not parse game id")?,
            size: parts[3]
                .parse::<u32>()
                .map_err(|_| "could not parse board size")?,
            opponent,
            color,
            time: (time, time),
            half_komi: parts[9]
                .parse::<u32>()
                .map_err(|_| "could not parse komi")?,
            flatstones: parts[10]
                .parse::<u32>()
                .map_err(|_| "could not parse flatstones")?,
            capstones: parts[11]
                .parse::<u32>()
                .map_err(|_| "could not parse capstones")?,
            ..Default::default()
        })
    }
}

impl Game {
    pub fn new_game_string(&self) -> String {
        format!("teinewgame {}\n", self.size)
    }

    pub fn search_string(&self) -> String {
        format!(
            "go wtime {} btime {}\n",
            self.time.0 * 1000,
            self.time.1 * 1000
        )
    }

    pub fn position_string(&self) -> String {
        let mut buffer = "position startpos moves".to_string();

        for game_move in &self.moves {
            write!(buffer, " {}", game_move.to_ptn()).unwrap();
        }

        writeln!(buffer).unwrap();

        buffer
    }
}

#[derive(Debug, Eq, PartialEq)]
pub enum GameMove {
    Place {
        x: u32,
        y: u32,
        piece_type: PieceType,
    },
    Spread {
        x: u32,
        y: u32,
        direction: Direction,
        drops: Vec<u32>,
    },
}

#[derive(Debug, Eq, PartialEq)]
pub enum PieceType {
    Flatstone,
    StandingStone,
    Capstone,
}

#[derive(Debug, Eq, PartialEq)]
pub enum Direction {
    North,
    South,
    East,
    West,
}

impl GameMove {
    pub fn from_playtak(value: &str) -> io::Result<Self> {
        let parts = value.split_ascii_whitespace().collect::<Vec<_>>();

        match parts[1] {
            "P" => {
                let (x, y) = coords_from_square(parts[2])?;

                let piece_type = match parts.get(3) {
                    None => PieceType::Flatstone,
                    Some(&"W") => PieceType::StandingStone,
                    Some(&"C") => PieceType::Capstone,
                    _ => return Err(err!("invalid piece type")),
                };

                Ok(Self::Place { x, y, piece_type })
            }
            "M" => {
                let (x, y) = coords_from_square(parts[2])?;
                let (tx, ty) = coords_from_square(parts[3])?;

                let direction = if ty > y {
                    Direction::North
                } else if ty < y {
                    Direction::South
                } else if tx > x {
                    Direction::East
                } else {
                    Direction::West
                };

                let mut drops = Vec::new();
                for drop in &parts[4..] {
                    drops.push(
                        drop.parse::<u32>()
                            .map_err(|_| err!("invalid drop amount"))?,
                    );
                }

                Ok(Self::Spread {
                    x,
                    y,
                    direction,
                    drops,
                })
            }
            _ => Err(err!("invalid move type")),
        }
    }

    pub fn to_playtak(&self, game_id: u32) -> String {
        match self {
            Self::Place { x, y, piece_type } => {
                let square = square_from_coords(*x, *y).to_uppercase();
                let mut buffer = format!("Game#{game_id} P {square}");

                match piece_type {
                    PieceType::StandingStone => write!(buffer, " W").unwrap(),
                    PieceType::Capstone => write!(buffer, " C").unwrap(),
                    _ => (),
                }

                writeln!(buffer).unwrap();

                buffer
            }
            Self::Spread {
                x,
                y,
                direction,
                drops,
            } => {
                let count = drops.len() as u32;
                let (tx, ty) = match direction {
                    Direction::North => (*x, *y + count),
                    Direction::South => (*x, *y - count),
                    Direction::East => (*x + count, *y),
                    Direction::West => (*x - count, *y),
                };

                let square = square_from_coords(*x, *y).to_uppercase();
                let target = square_from_coords(tx, ty).to_uppercase();

                let mut buffer = format!("Game#{game_id} M {square} {target}");

                for drop in drops {
                    write!(buffer, " {drop}").unwrap();
                }

                writeln!(buffer).unwrap();

                buffer
            }
        }
    }

    pub fn from_ptn(value: &str) -> io::Result<Self> {
        let mut chars = value.chars().collect::<Vec<_>>();

        let piece_type = match chars[0] {
            'S' => PieceType::StandingStone,
            'C' => PieceType::Capstone,
            _ => PieceType::Flatstone,
        };

        let pickup = chars[0].to_digit(10).unwrap_or(1);

        if ['F', 'S', 'C'].contains(&chars[0]) || chars[0].is_ascii_digit() {
            chars.remove(0);
        }

        let (x, y) = coords_from_square(&format!("{}{}", chars[0], chars[1]))?;

        if chars.len() == 2 {
            Ok(GameMove::Place { x, y, piece_type })
        } else if chars.len() >= 3 {
            let direction = match chars[2] {
                '+' => Direction::North,
                '-' => Direction::South,
                '>' => Direction::East,
                '<' => Direction::West,
                _ => return Err(err!("invalid direction character")),
            };

            let mut drops = Vec::new();
            for c in &chars[3..] {
                if let Some(drop) = c.to_digit(10) {
                    drops.push(drop);
                } else {
                    break;
                }
            }

            if drops.is_empty() {
                drops.push(pickup);
            }

            Ok(GameMove::Spread {
                x,
                y,
                direction,
                drops,
            })
        } else {
            Err(err!("ptn move is too short"))
        }
    }

    pub fn to_ptn(&self) -> String {
        match self {
            Self::Place { x, y, piece_type } => {
                let square = square_from_coords(*x, *y);
                format!(
                    "{}{square}",
                    match piece_type {
                        PieceType::Flatstone => "",
                        PieceType::StandingStone => "S",
                        PieceType::Capstone => "C",
                    }
                )
            }
            Self::Spread {
                x,
                y,
                direction,
                drops,
            } => {
                let mut buffer = match drops.iter().sum::<u32>() {
                    n if n > 1 => format!("{n}"),
                    _ => String::new(),
                };

                buffer.write_str(&square_from_coords(*x, *y)).unwrap();

                buffer
                    .write_char(match direction {
                        Direction::North => '+',
                        Direction::South => '-',
                        Direction::East => '>',
                        Direction::West => '<',
                    })
                    .unwrap();

                if drops.len() > 1 {
                    for drop in drops {
                        buffer
                            .write_char(char::from_digit(*drop, 10).unwrap())
                            .unwrap();
                    }
                }

                buffer
            }
        }
    }
}

fn coords_from_square(value: &str) -> io::Result<(u32, u32)> {
    if value.len() != 2 {
        return Err(err!("invalid space"));
    }

    let mut chars = value.chars();

    let file_number = chars
        .next()
        .unwrap()
        .to_digit(18)
        .filter(|&f| f >= 10)
        .ok_or_else(|| err!("invalid file letter"))?
        - 10;

    let rank_number = chars
        .next()
        .unwrap()
        .to_digit(10)
        .filter(|&r| r >= 1)
        .ok_or_else(|| err!("invalid rank number"))?
        - 1;

    Ok((file_number, rank_number))
}

fn square_from_coords(x: u32, y: u32) -> String {
    format!(
        "{}{}",
        char::from_digit(x + 10, 18).unwrap(),
        char::from_digit(y + 1, 10).unwrap(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn move_from_playtak() {
        let game_move = GameMove::from_playtak("Game#123456 P A1").unwrap();
        assert_eq!(
            game_move,
            GameMove::Place {
                x: 0,
                y: 0,
                piece_type: PieceType::Flatstone
            },
        );

        let game_move = GameMove::from_playtak("Game#123456 P C6 C").unwrap();
        assert_eq!(
            game_move,
            GameMove::Place {
                x: 2,
                y: 5,
                piece_type: PieceType::Capstone
            },
        );

        let game_move = GameMove::from_playtak("Game#123456 M B4 F4 2 1 2 1").unwrap();
        assert_eq!(
            game_move,
            GameMove::Spread {
                x: 1,
                y: 3,
                direction: Direction::East,
                drops: vec![2, 1, 2, 1],
            },
        );
    }

    #[test]
    fn move_to_playtak() {
        let game_move = GameMove::Place {
            x: 0,
            y: 0,
            piece_type: PieceType::Flatstone,
        }
        .to_playtak(123456);
        assert_eq!(game_move, "Game#123456 P A1\n",);

        let game_move = GameMove::Place {
            x: 2,
            y: 5,
            piece_type: PieceType::Capstone,
        }
        .to_playtak(123456);
        assert_eq!(game_move, "Game#123456 P C6 C\n",);

        let game_move = GameMove::Spread {
            x: 1,
            y: 3,
            direction: Direction::East,
            drops: vec![2, 1, 2, 1],
        }
        .to_playtak(123456);
        assert_eq!(game_move, "Game#123456 M B4 F4 2 1 2 1\n",);
    }

    #[test]
    fn move_from_ptn() {
        let game_move = GameMove::from_ptn("a1").unwrap();
        assert_eq!(
            game_move,
            GameMove::Place {
                x: 0,
                y: 0,
                piece_type: PieceType::Flatstone
            },
        );

        let game_move = GameMove::from_ptn("Sc5").unwrap();
        assert_eq!(
            game_move,
            GameMove::Place {
                x: 2,
                y: 4,
                piece_type: PieceType::StandingStone
            },
        );

        let game_move = GameMove::from_ptn("b4+").unwrap();
        assert_eq!(
            game_move,
            GameMove::Spread {
                x: 1,
                y: 3,
                direction: Direction::North,
                drops: vec![1]
            },
        );

        let game_move = GameMove::from_ptn("3b4+").unwrap();
        assert_eq!(
            game_move,
            GameMove::Spread {
                x: 1,
                y: 3,
                direction: Direction::North,
                drops: vec![3]
            },
        );

        let game_move = GameMove::from_ptn("5b2>122").unwrap();
        assert_eq!(
            game_move,
            GameMove::Spread {
                x: 1,
                y: 1,
                direction: Direction::East,
                drops: vec![1, 2, 2]
            },
        );

        let game_move = GameMove::from_ptn("5f2<221*").unwrap();
        assert_eq!(
            game_move,
            GameMove::Spread {
                x: 5,
                y: 1,
                direction: Direction::West,
                drops: vec![2, 2, 1]
            },
        );
    }

    #[test]
    fn move_to_ptn() {
        let game_move = GameMove::Place {
            x: 0,
            y: 0,
            piece_type: PieceType::Flatstone,
        }
        .to_ptn();
        assert_eq!(game_move, "a1",);

        let game_move = GameMove::Place {
            x: 2,
            y: 4,
            piece_type: PieceType::StandingStone,
        }
        .to_ptn();
        assert_eq!(game_move, "Sc5",);

        let game_move = GameMove::Spread {
            x: 1,
            y: 3,
            direction: Direction::North,
            drops: vec![1],
        }
        .to_ptn();
        assert_eq!(game_move, "b4+",);

        let game_move = GameMove::Spread {
            x: 1,
            y: 3,
            direction: Direction::North,
            drops: vec![3],
        }
        .to_ptn();
        assert_eq!(game_move, "3b4+",);

        let game_move = GameMove::Spread {
            x: 1,
            y: 1,
            direction: Direction::East,
            drops: vec![1, 2, 2],
        }
        .to_ptn();
        assert_eq!(game_move, "5b2>122",);

        let game_move = GameMove::Spread {
            x: 5,
            y: 1,
            direction: Direction::West,
            drops: vec![2, 2, 1],
        }
        .to_ptn();
        assert_eq!(game_move, "5f2<221",);
    }
}
