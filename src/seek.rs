use std::fmt;
use std::str::FromStr;

use clap::ValueEnum;
use clap::{arg, Args};

#[derive(Args, Clone, Debug)]
pub struct Seek {
    #[arg(skip)]
    pub id: Option<u32>,
    #[arg(skip)]
    pub player: Option<String>,
    #[arg(short, long, value_parser = clap::value_parser!(u32).range(3..=8))]
    pub size: u32,
    #[arg(short = 'm', long, default_value_t = 1200)]
    pub time: u32,
    #[arg(short, long, default_value_t = 20)]
    pub increment: u32,
    #[arg(short, long, value_enum, default_value_t = SeekColor::Random)]
    pub color: SeekColor,
    #[arg(short = 'k', long, default_value_t = 0)]
    pub half_komi: u32,
    #[arg(long)]
    flatstones: Option<u32>,
    #[arg(long)]
    capstones: Option<u32>,
    #[arg(long, action)]
    pub unrated: bool,
    #[arg(long, action)]
    pub tournament: bool,
    #[arg(long)]
    pub extra_time_move: Option<u32>,
    #[arg(long)]
    pub extra_time_amount: Option<u32>,
    #[arg(short, long)]
    pub opponent: Option<String>,
}

impl Seek {
    pub fn flatstones(&self) -> u32 {
        self.flatstones
            .unwrap_or_else(|| flatstones_for_size(self.size))
    }

    pub fn capstones(&self) -> u32 {
        self.capstones
            .unwrap_or_else(|| capstones_for_size(self.size))
    }
}

impl FromStr for Seek {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let parts = s.split_ascii_whitespace().collect::<Vec<_>>();

        Ok(Self {
            id: Some(
                parts[2]
                    .parse::<u32>()
                    .map_err(|_| "could not parse seek number")?,
            ),
            player: Some(parts[3].to_owned()),
            size: parts[4]
                .parse::<u32>()
                .map_err(|_| "could not parse board size")?,
            time: parts[5]
                .parse::<u32>()
                .map_err(|_| "could not parse time")?,
            increment: parts[6]
                .parse::<u32>()
                .map_err(|_| "could not parse increment")?,
            color: match parts[7] {
                "W" => SeekColor::White,
                "B" => SeekColor::Black,
                "A" => SeekColor::Random,
                _ => panic!("invalid seeker color"),
            },
            half_komi: parts[8]
                .parse::<u32>()
                .map_err(|_| "could not parse half komi")?,
            flatstones: Some(
                parts[9]
                    .parse::<u32>()
                    .map_err(|_| "could not parse flatstones")?,
            ),
            capstones: Some(
                parts[10]
                    .parse::<u32>()
                    .map_err(|_| "could not parse capstones")?,
            ),
            unrated: match parts[11] {
                "0" => false,
                "1" => true,
                _ => panic!("invalid unrated value"),
            },
            tournament: match parts[12] {
                "0" => false,
                "1" => true,
                _ => panic!("invalid tournament value"),
            },
            extra_time_move: Some(
                parts[13]
                    .parse::<u32>()
                    .map_err(|_| "could not parse extra time move")?,
            )
            .filter(|&v| v > 0),
            extra_time_amount: Some(
                parts[14]
                    .parse::<u32>()
                    .map_err(|_| "could not parse extra time amount")?,
            )
            .filter(|&v| v > 0),
            opponent: parts.get(15).map(|&o| o.to_owned()),
        })
    }
}

impl Seek {
    pub fn to_seek_string(&self) -> String {
        format!(
            "Seek {} {} {} {} {} {} {} {} {} {} {} {}\n",
            self.size,
            self.time,
            self.increment,
            match self.color {
                SeekColor::White => "W",
                SeekColor::Black => "B",
                SeekColor::Random => "A",
            },
            self.half_komi,
            self.flatstones(),
            self.capstones(),
            match self.unrated {
                false => 0,
                true => 1,
            },
            match self.tournament {
                false => 0,
                true => 1,
            },
            self.extra_time_move.unwrap_or_default(),
            self.extra_time_amount.unwrap_or_default(),
            self.opponent.as_deref().unwrap_or(""),
        )
    }
}

impl fmt::Display for Seek {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "  Seek")?;

        match self.id {
            Some(id) => write!(f, " {id}: ")?,
            None => write!(f, ": ")?,
        }

        match &self.player {
            Some(player) => writeln!(f, "{player}")?,
            None => writeln!(f)?,
        }

        write!(
            f,
            "      size: {}, seeker color: {}, time: {:}, komi: {}",
            self.size,
            match self.color {
                SeekColor::White => "white",
                SeekColor::Black => "black",
                SeekColor::Random => "random",
            },
            format_args!("{}+{}", self.time, self.increment),
            format_args!("{:3.1}", self.half_komi as f32 / 2.0),
        )?;

        if self.flatstones() != flatstones_for_size(self.size) {
            write!(f, ", flatstones: {}", self.flatstones())?;
        }

        if self.capstones() != capstones_for_size(self.size) {
            write!(f, ", capstones: {}", self.capstones())?;
        }

        if self.unrated {
            write!(f, ", unrated")?;
        }

        if self.tournament {
            write!(f, ", tournament")?;
        }

        if let Some(opponent) = &self.opponent {
            write!(f, ", opponent: {opponent}")?;
        }

        Ok(())
    }
}

#[derive(Clone, Debug, ValueEnum)]
pub enum SeekColor {
    White,
    Black,
    Random,
}

pub fn flatstones_for_size(size: u32) -> u32 {
    match size {
        3 => 10,
        4 => 15,
        5 => 21,
        6 => 30,
        7 => 40,
        8 => 50,
        _ => unreachable!(),
    }
}

pub fn capstones_for_size(size: u32) -> u32 {
    match size {
        3 => 0,
        4 => 0,
        5 => 1,
        6 => 1,
        7 => 2,
        8 => 2,
        _ => unreachable!(),
    }
}
