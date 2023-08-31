#[macro_use]
extern crate tracing;
#[macro_use]
extern crate serde;

pub mod fix;
pub mod melt;
pub mod mint;
pub mod opts;
pub mod recv;
pub mod restore;
pub mod send;
pub mod show;

use std::sync::Arc;

use opts::{Cli, Commands, Parser};

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    let verbose = cli.verbose();

    if std::env::var("RUST_LOG").is_ok() {
        tracing_subscriber::fmt().with_line_number(true).init();
    } else {
        tracing_subscriber::fmt()
            .with_max_level(verbose.to_log())
            .with_line_number(true)
            .init()
    }

    use cashu_wallet::store::impl_redb::Redb;
    use cashu_wallet::wallet::HttpOptions;
    use cashu_wallet::UnitedWallet;
    use cashu_wallet_sqlite::LitePool;

    let mut mnemonic = None;
    if cli.words() != "" {
        let m = cashu_wallet::wallet::MnemonicInfo::with_words(cli.words())
            .expect("invalid mnemonic words");
        mnemonic = Some(Arc::new(m));
    }

    macro_rules! call {
        ($opts: expr) => {{
            let dburl = $opts.database.as_str();
            let timeout = $opts.timeout;

            let c = HttpOptions::new()
                .connection_verbose(true)
                .timeout_connect_ms(3000)
                .timeout_swap_ms(timeout);

            if dburl.ends_with(".redb") || dburl.ends_with(".red") {
                let db = Redb::open(dburl, Default::default()).unwrap();
                let w = UnitedWallet::new(db, c);
                $opts.run(w).await
            } else if dburl.ends_with(".sqlite")
                || dburl.ends_with(".sqlite3")
                || dburl.ends_with(".db")
            {
                let db = LitePool::open(dburl, Default::default()).await.unwrap();
                let w = UnitedWallet::with_mnemonic(db, c, mnemonic);
                $opts.run(w).await
            } else {
                panic!("unsupport database path/url")
            }
        }};
    }

    match cli.command {
        Commands::Recv(c) => {
            call!(c)
        }
        Commands::Send(c) => {
            call!(c)
        }
        Commands::Show(c) => {
            call!(c)
        }
        Commands::Fix(c) => {
            call!(c)
        }
        Commands::Mint(c) => {
            call!(c)
        }
        Commands::Melt(c) => {
            call!(c)
        }
        Commands::Restore(c) => {
            call!(c)
        }
    }
}
