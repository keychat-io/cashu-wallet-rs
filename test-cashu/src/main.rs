#[macro_use]
extern crate tracing;
#[macro_use]
extern crate serde;

use std::time::Instant;

use cashu_wallet::store::UnitedStore;
use cashu_wallet::types::TransactionStatus;
use cashu_wallet::wallet::{HttpOptions, MintClient};
use cashu_wallet::{UnitedWallet, Url};

#[tokio::main]
async fn main() {
    if std::env::var("RUST_LOG").is_ok() {
        tracing_subscriber::fmt().with_line_number(true).init();
    } else {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::INFO)
            .with_line_number(true)
            .init()
    }

    // println!("uni: {}", std::mem::size_of::<cashu_wallet::UniError<cashu_wallet::store::impl_redb::StoreError>>());
    // println!("sto: {}", std::mem::size_of::<cashu_wallet::store::impl_redb::StoreError>());
    // println!("dab: {}", std::mem::size_of::<cashu_wallet::store::impl_redb::redb::DatabaseError>());
    // println!("red: {}", std::mem::size_of::<cashu_wallet::store::impl_redb::redb::Error>());
    // println!("tab: {}", std::mem::size_of::<cashu_wallet::store::impl_redb::redb::TableError>());
    // println!("wal: {}", std::mem::size_of::<cashu_wallet::wallet::WalletError>());
    // println!("cli: {}", std::mem::size_of::<cashu_wallet::wallet::ClientError>());
    // println!("how: {}", std::mem::size_of::<anyhow::Error>());
    // println!("Str: {}", std::mem::size_of::<String>());
    // println!("u64: {}", std::mem::size_of::<u64>());

    let mint_url = cashu_wallet::types::tests::MINT_URL_TEST;
    let mint_url = mint_url.parse::<Url>().unwrap();

    let c = HttpOptions::new()
        .connection_verbose(true)
        .timeout_connect_ms(3000)
        .timeout_swap_ms(5000);
    let client = MintClient::new(mint_url.clone(), c.clone()).unwrap();

    let mint_keysets = client.get_info().await.unwrap();
    info!("{:?}", mint_keysets);

    // let mint_keys = client.get_mint_keys(&mint_url).await.unwrap();
    // println!("mint_keys.len: {:?}", mint_keys.len());

    // let db = cashu_wallet::store::impl_redb::Redb::open("uni.redb", Default::default()).unwrap();
    let db = cashu_wallet_sqlite::LitePool::open("uni.sqlite", Default::default())
        .await
        .unwrap();
    let w = UnitedWallet::new(db, c);

    println!("add_mint: {:?}", w.add_mint(mint_url.clone(), false).await);

    let mint = w.load_mints_from_database().await.unwrap();
    println!("load_mints: {:?}", mint);
    // if let Some(m) = mint.first_mut() {
    //     m.active = false;

    //     println!("update_mint: {:?}", w.update_mint(&*m).await);
    // }

    println!("get_balance: {:?}", w.get_balance(&mint_url).await);

    let tokens = r#"
    cashuAeyJ0b2tlbiI6W3sicHJvb2ZzIjpbeyJhbW91bnQiOjgsInNlY3JldCI6IjZiYzFiMzY0MDFjODcyYjQzNzk4MjkyMzNkNWU2MmZiYTA3YTM3MzZkMmQzYzg4MGM0YzBmZTg0OTg0MzgyMjMiLCJDIjoiMDM0YzU3ZjVjZTE5YTMzZDRlYTQ4MjI5ODY4ZjVlYmQ2OTZhZjk0ODZhODgwOTI1YjlhYTc4YmQ0MmYwMzMwMGUwIiwicmVzZXJ2ZWQiOmZhbHNlLCJpZCI6IjAwOWExZjI5MzI1M2U0MWUifSx7ImFtb3VudCI6NCwic2VjcmV0IjoiN2NmMjBmNTkyNzg0NTI4OTZlYmRiMTljNDI1NGQxMjc0N2Y0NDk5OWFkZmViM2RlMzU5Nzk4YmJjNzZlMWZiYyIsIkMiOiIwMmM3NjdhNDJlYjAwMWM3MWY5YjY4N2RlNzhlNGQ3ZGFiZTBiYjJkMjBkNmU0ZDEzYzUzMTFiMjExZDE4ZGNkMWQiLCJyZXNlcnZlZCI6ZmFsc2UsImlkIjoiMDA5YTFmMjkzMjUzZTQxZSJ9LHsiYW1vdW50IjoyLCJzZWNyZXQiOiI0MjNkMjNjNjkzYjdiNzlhMDQ4NjIzM2Y4OThhN2QyYjEzZGIyNWJmYWQzNWNhYWRjMTFjOTA3ZDZiNjdmMDQzIiwiQyI6IjAzMGRhNDdlZWRiMjk5NTU0NjFhYzY1MmZhOWY5MjNhNDk0M2FhMjZkZDAyZGI1NDNmNGIyZmNlNmUyMjkwNDE3ZSIsInJlc2VydmVkIjpmYWxzZSwiaWQiOiIwMDlhMWYyOTMyNTNlNDFlIn0seyJhbW91bnQiOjEsInNlY3JldCI6IjlhMDM0MDlhNzdjNTIzNDgxZjJhZGQyZDFiNDEyNTU3MWYzM2NjNDE4YmY4MjU3Yjc3YzI2OTU0ZDY3YWU0NmQiLCJDIjoiMDNmZmU0ZTQ0OWNkYzllNjk5MjJmMzhhZDcwNjI1MjZkMDVkNTM5Y2NiNmIwODk4ODc2YWM4YmVkYTk5MGQyZDA1IiwicmVzZXJ2ZWQiOmZhbHNlLCJpZCI6IjAwOWExZjI5MzI1M2U0MWUifV0sIm1pbnQiOiJodHRwczovL3Rlc3RudXQuY2FzaHUuc3BhY2UifV0sInVuaXQiOiJzYXQifQ==
    "#.trim();

    let start = Instant::now();
    let receive = w.receive_tokens(tokens).await;
    println!("receive_tokens {:?}: {:?}", start.elapsed(), receive);

    // let mr = w.request_mint(&mint_url, 10_0000).await;
    // println!("request_mint {:?}: {:?}", start.elapsed(), mr);
    // return;

    // let tokens = r#"
    // cashuAeyJ0b2tlbiI6W3sibWludCI6Imh0dHBzOi8vODMzMy5zcGFjZTozMzM4IiwicHJvb2ZzIjpbeyJhbW91bnQiOjEsInNlY3JldCI6IjcxMFdXQ05xZzFPZDR1cjM3VDIxcmtNMyIsIkMiOiIwMzI0MThlNDk2NWIyOTQwNGM1ZDg4ZmI4YWQ0NDNhYzFlZTkxNmM0MTJlZjhjZmQxYzNkZGIyMmFiNTRlZjc5OTIiLCJpZCI6IkkyeU4raVJZZmt6VCJ9XX1dfQ==
    // "#.trim();
    // println!("receive_tokens: {:?}", w.receive_tokens(tokens).await);

    println!("get_balance: {:?}", w.get_balance(&mint_url).await);
    println!("get_balances: {:?}", w.get_balances().await);

    println!(
        "delete_proofs: {:?}",
        w.store().delete_proofs(&mint_url, &Vec::new()).await
    );

    println!(
        "prepare_ones: {:?}",
        w.prepare_one_proofs(&mint_url, 3, None).await
    );

    println!(
        "prepare_ones: {:?}",
        w.prepare_one_proofs(&mint_url, 9, None).await
    );

    println!(
        "prepare_ones: {:?}",
        w.prepare_one_proofs(&mint_url, 1, None).await
    );

    // let send = w.send_tokens_full(&mint_url, 30, None, false).await;
    // match send {
    //     Ok(t) => println!("send {}: {:?}", t.amount(), t.content()),
    //     Err(e) => println!("send failed: {}", e),
    // }

    let send = w.send_tokens(&mint_url, 20, None, None, None).await;
    match send {
        Ok(t) => println!("send {}: {:?}", t.amount(), t.content()),
        Err(e) => println!("send failed: {}", e),
    }

    // info!(
    //     "check_proofs_in_database: {:?}",
    //     w.check_proofs_in_database().await
    // );

    println!("get_balance: {:?}", w.get_balance(&mint_url).await);
    println!("get_mints: {:?}", w.store().get_mints().await);

    get_txs(&w).await;
    let res = w
        .store()
        .delete_transactions(
            [
                TransactionStatus::Success,
                TransactionStatus::Failed,
                TransactionStatus::Expired,
            ]
            .as_slice(),
            1694758654680,
        )
        .await;
    println!("delete_transactions: {:?}\n", res);

    info!("check_pendings: {:?}\n", w.check_pendings().await);

    get_txs(&w).await;
    println!("get_balance: {:?}", w.get_balance(&mint_url).await);
}

async fn get_txs<S>(w: &UnitedWallet<S>)
where
    S: UnitedStore + Clone + Send + Sync,
{
    let b = false;

    if b {
        let mut pendings = vec![];

        match w.store().get_all_transactions().await {
            Err(e) => println!("get_all_transactions failed: {:?}", e),
            Ok(mut txs) => {
                println!("get_all_transactions ok.len: {:?}", txs.len());

                txs.sort_by_key(|a| a.time());

                for (idx, mut tx) in txs.into_iter().enumerate() {
                    println!(
                        "{:>2} {}: {:>3} {:>7} {} {}",
                        idx,
                        tx.time(),
                        tx.direction().as_ref(),
                        tx.status().as_ref(),
                        tx.amount(),
                        tx.id()
                    );

                    if tx.is_pending() {
                        pendings.push(tx.content().to_owned());
                    }

                    *tx.status_mut() = TransactionStatus::Pending;
                    // w.store().add_transaction(&tx).await.unwrap();
                }
            }
        }

        for (i, tx) in pendings.into_iter().enumerate() {
            println!("{:>2}: {}", i, tx)
        }
    }
}
