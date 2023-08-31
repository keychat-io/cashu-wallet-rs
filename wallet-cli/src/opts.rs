pub use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[clap(author, version, about, long_about = None)]
#[clap(propagate_version = true)]
pub struct Cli {
    #[clap(subcommand)]
    pub command: Commands,
}

use tracing::Level as LevelFilter;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Verbose(u8);
impl Verbose {
    pub fn to_log(&self) -> LevelFilter {
        match self.0 {
            0 => LevelFilter::INFO,
            1 => LevelFilter::DEBUG,
            _more => LevelFilter::TRACE,
        }
    }
    pub fn isdebug(&self) -> bool {
        self.0 > 0
    }
}

impl Cli {
    pub fn verbose(&self) -> Verbose {
        let v = match &self.command {
            Commands::Show(c) => c.verbose,
            Commands::Recv(c) => c.verbose,
            Commands::Send(c) => c.verbose,
            Commands::Fix(c) => c.verbose,
            Commands::Mint(c) => c.verbose,
            Commands::Melt(c) => c.verbose,
            Commands::Restore(c) => c.verbose,
        };
        Verbose(v)
    }
    pub fn words(&self) -> &str {
        let v = match &self.command {
            Commands::Show(c) => &c.words,
            Commands::Recv(c) => &c.words,
            Commands::Send(c) => &c.words,
            Commands::Fix(_c) => "",
            Commands::Mint(c) => &c.words,
            Commands::Melt(c) => &c.words,
            Commands::Restore(c) => &c.words,
        };
        v
    }
}

#[derive(Subcommand)]
pub enum Commands {
    Show(ShowOpts),
    Recv(RecvOpts),
    Send(SendOpts),
    Fix(FixOpts),
    Mint(MintOpts),
    Melt(MeltOpts),
    Restore(RestoreOpts),
}

#[derive(Args, Debug, Clone)]
// #[clap(version = env!("CARGO_PKG_VERSION"))]
// #[clap(help = "Show balance and txs")]
pub struct ShowOpts {
    #[clap(short, long, default_value = "uni.redb", help = "The path of databse")]
    pub database: String,
    #[arg(
            long,
            short = 'v',
            action = clap::ArgAction::Count,
            global = true,
            help = "Loglevel: -v(Info), -vv(Debug), -vvv+(Trace)"
        )]
    pub verbose: u8,
    #[clap(short, long, default_value = "5000", help = "timeout millis")]
    pub timeout: u64,
    #[clap(short, long, help = "check pendings")]
    pub check: bool,
    #[clap(short, long, help = "recycle pendings")]
    pub recycle: bool,
    #[clap(short = 'T', long, help = "show transactions")]
    pub transactions: bool,
    #[clap(short, long, help = "show proofs")]
    pub proofs: bool,
    #[clap(
        short,
        long,
        default_value = "100",
        help = "the number of limit for show txs"
    )]
    pub limit: usize,
    #[clap(
        short,
        long,
        default_value = "",
        help = "only restore for the mnmonic words"
    )]
    pub words: String,
}

#[derive(Args, Debug, Clone)]
// #[clap(help = "Recv tokens")]
pub struct RecvOpts {
    #[clap(
        short,
        long,
        default_value = "https://8333.space:3338/",
        help = "The url of mint"
    )]
    pub mint: String,
    #[clap(short, long, default_value = "uni.redb", help = "The path of databse")]
    pub database: String,
    #[arg(
        long,
        short = 'v',
        action = clap::ArgAction::Count,
        global = true,
        help = "Loglevel: -v(Info), -vv(Debug), -vvv+(Trace)"
    )]
    pub verbose: u8,
    #[clap(short, long, default_value = "5000", help = "timeout millis")]
    pub timeout: u64,
    #[clap(value_parser, help = "base64 encoded token")]
    pub tokens: Vec<String>,
    #[clap(short, long, help = "try for per coin in tokens")]
    pub percoin: bool,
    #[clap(
        short,
        long,
        default_value = "",
        help = "only restore for the mnmonic words"
    )]
    pub words: String,
}

#[derive(Args, Debug, Clone)]
// #[clap(help = "Send value")]
pub struct SendOpts {
    #[clap(
        short,
        long,
        default_value = "https://8333.space:3338/",
        help = "The url of mint"
    )]
    pub mint: String,
    #[clap(short, long, default_value = "uni.redb", help = "The path of databse")]
    pub database: String,
    #[arg(
        long,
        short = 'v',
        action = clap::ArgAction::Count,
        global = true,
        help = "Loglevel: -v(Info), -vv(Debug), -vvv+(Trace)"
    )]
    pub verbose: u8,
    #[clap(short, long, default_value = "5000", help = "timeout millis")]
    pub timeout: u64,
    #[clap(
        long,
        default_value = "0",
        help = "send the value, zero is meaning all"
    )]
    pub value: u64,
    #[clap(short, long, default_value = "64", help = "the number limit for coins")]
    pub limit: u64,
    #[clap(long, default_value = "sat", help = "currency unit")]
    pub unit: String,
    #[clap(
        short,
        long,
        default_value = "",
        help = "only restore for the mnmonic words"
    )]
    pub words: String,
}

#[derive(Args, Debug, Clone)]
// #[clap(help = "Check or fix proofs in database")]
pub struct FixOpts {
    #[clap(short, long, default_value = "uni.redb", help = "The path of databse")]
    pub database: String,
    #[arg(
        long,
        short = 'v',
        action = clap::ArgAction::Count,
        global = true,
        help = "Loglevel: -v(Info), -vv(Debug), -vvv+(Trace)"
    )]
    pub verbose: u8,
    #[clap(short, long, default_value = "5000", help = "timeout millis")]
    pub timeout: u64,
    #[clap(short, long, help = "write to db")]
    pub write: bool,
}

#[derive(Args, Debug, Clone)]
// #[clap(help = "Send value")]
pub struct MintOpts {
    #[clap(
        short,
        long,
        default_value = "https://8333.space:3338/",
        help = "The url of mint"
    )]
    pub mint: String,
    #[clap(short, long, default_value = "uni.redb", help = "The path of databse")]
    pub database: String,
    #[arg(
        long,
        short = 'v',
        action = clap::ArgAction::Count,
        global = true,
        help = "Loglevel: -v(Info), -vv(Debug), -vvv+(Trace)"
    )]
    pub verbose: u8,
    #[clap(short, long, default_value = "5000", help = "timeout millis")]
    pub timeout: u64,
    #[clap(
        long,
        default_value = "0",
        help = "mint the value, zero is meaning to update for history mint quote"
    )]
    pub value: u64,
    #[clap(long, default_value = "sat", help = "currency unit")]
    pub unit: String,
    #[clap(
        short,
        long,
        default_value = "",
        help = "only restore for the mnmonic words"
    )]
    pub words: String,
}

#[derive(Args, Debug, Clone)]
// #[clap(help = "Send value")]
pub struct MeltOpts {
    #[clap(
        short,
        long,
        default_value = "https://8333.space:3338/",
        help = "The url of mint"
    )]
    pub mint: String,
    #[clap(short, long, default_value = "uni.redb", help = "The path of databse")]
    pub database: String,
    #[arg(
        long,
        short = 'v',
        action = clap::ArgAction::Count,
        global = true,
        help = "Loglevel: -v(Info), -vv(Debug), -vvv+(Trace)"
    )]
    pub verbose: u8,
    #[clap(short, long, default_value = "5000", help = "timeout millis")]
    pub timeout: u64,
    #[clap(short, long, help = "the bolt11 Lightning invoice")]
    pub request: String,
    // #[clap(short, long, help = "real pay the invoice")]
    // pub pay: bool,
    #[clap(long, default_value = "sat", help = "currency unit")]
    pub unit: String,
    #[clap(
        short,
        long,
        default_value = "",
        help = "only restore for the mnmonic words"
    )]
    pub words: String,
}

#[derive(Args, Debug, Clone)]
// #[clap(help = "Send value")]
pub struct RestoreOpts {
    #[clap(
        short,
        long,
        default_value = "https://8333.space:3338/",
        help = "The url of mint"
    )]
    pub mint: String,
    #[clap(short, long, default_value = "uni.redb", help = "The path of databse")]
    pub database: String,
    #[arg(
        long,
        short = 'v',
        action = clap::ArgAction::Count,
        global = true,
        help = "Loglevel: -v(Info), -vv(Debug), -vvv+(Trace)"
    )]
    pub verbose: u8,
    #[clap(short, long, default_value = "5000", help = "timeout millis")]
    pub timeout: u64,
    #[clap(short, long, default_value = "1", help = "sleepms after check a batch")]
    pub sleepms: u64,
    #[clap(short, long, default_value = "10", help = "batch size for restore")]
    pub batch: u64,
    #[clap(
        short,
        long,
        default_value = "",
        help = "only restore for the keysetid"
    )]
    pub keysetid: String,
    #[clap(
        short,
        long,
        default_value = "",
        help = "only restore for the mnmonic words"
    )]
    pub words: String,
}
