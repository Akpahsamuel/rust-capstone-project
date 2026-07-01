#![allow(unused)]
use bitcoin::hex::DisplayHex;
use bitcoincore_rpc::bitcoin::Amount;
use bitcoincore_rpc::{Auth, Client, RpcApi};
use serde::Deserialize;
use serde_json::json;
use std::fs::File;
use std::io::Write;

// Node access params
const RPC_URL: &str = "http://127.0.0.1:18443"; // Default regtest RPC port
const RPC_USER: &str = "alice";
const RPC_PASS: &str = "password";

// You can use calls not provided in RPC lib API using the generic `call` function.
// An example of using the `send` RPC call, which doesn't have exposed API.
// You can also use serde_json `Deserialize` derivation to capture the returned json result.
fn send(rpc: &Client, addr: &str) -> bitcoincore_rpc::Result<String> {
    let args = [
        json!([{addr : 100 }]), // recipient address
        json!(null),            // conf target
        json!(null),            // estimate mode
        json!(null),            // fee rate in sats/vb
        json!(null),            // Empty option object
    ];

    #[derive(Deserialize)]
    struct SendResult {
        complete: bool,
        txid: String,
    }
    let send_result = rpc.call::<SendResult>("send", &args)?;
    assert!(send_result.complete);
    Ok(send_result.txid)
}

/// Build an RPC client scoped to a specific wallet.
///
/// Bitcoin Core exposes per-wallet RPCs (getbalance, sendtoaddress, ...) under
/// the `/wallet/<name>` URL path, so each wallet needs its own client.
fn wallet_client(name: &str) -> bitcoincore_rpc::Result<Client> {
    Client::new(
        &format!("{RPC_URL}/wallet/{name}"),
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )
}

/// Ensure a wallet with the given name exists and is loaded.
///
/// This is idempotent so the program can be re-run against a persistent node:
/// we first try to create the wallet; if that fails (it already exists on
/// disk) we fall back to loading it, and if it is already loaded we ignore the
/// resulting error.
fn ensure_wallet(rpc: &Client, name: &str) {
    if rpc
        .create_wallet(name, None, None, None, None)
        .is_ok()
    {
        return;
    }
    // Already exists on disk (or already loaded) — try to load, ignore if loaded.
    let _ = rpc.load_wallet(name);
}

fn main() -> bitcoincore_rpc::Result<()> {
    // Connect to Bitcoin Core RPC (node-level client, not tied to a wallet).
    let rpc = Client::new(
        RPC_URL,
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )?;

    // Get blockchain info
    let blockchain_info = rpc.get_blockchain_info()?;
    println!("Blockchain Info: {:?}", blockchain_info);

    // ---- Phase 2: Create/Load the 'Miner' and 'Trader' wallets ----
    // Names are case-sensitive and must match exactly.
    ensure_wallet(&rpc, "Miner");
    ensure_wallet(&rpc, "Trader");

    // Per-wallet clients used for all wallet-scoped calls below.
    let miner = wallet_client("Miner")?;
    let trader = wallet_client("Trader")?;
    println!("Wallets ready: Miner and Trader loaded.");

    // Generate spendable balances in the Miner wallet. How many blocks needs to be mined?

    // Load Trader wallet and generate a new address

    // Send 20 BTC from Miner to Trader

    // Check transaction in mempool

    // Mine 1 block to confirm the transaction

    // Extract all required transaction details

    // Write the data to ../out.txt in the specified format given in readme.md

    Ok(())
}
