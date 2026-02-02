//! Prediction Market CLI
//!
//! A CLI for interacting with the Theseus Prediction Market demo.
//! Connects directly to the chain via subxt.

use anyhow::{anyhow, Context, Result};
use clap::{Parser, Subcommand};
use codec::Encode;
use console::{style, Emoji};
use dialoguer::{theme::ColorfulTheme, Confirm, Input, Select};
use subxt::{dynamic::Value, OnlineClient, PolkadotConfig};
use subxt_signer::sr25519::Keypair;

static CRYSTAL_BALL: Emoji<'_, '_> = Emoji("🔮 ", "");
static CHECK: Emoji<'_, '_> = Emoji("✅ ", "[OK] ");
static MONEY: Emoji<'_, '_> = Emoji("💰 ", "$");
static CLOCK: Emoji<'_, '_> = Emoji("⏰ ", "");

type TheseusConfig = PolkadotConfig;

#[derive(Parser)]
#[command(name = "pm")]
#[command(about = "Prediction Market CLI - Create and resolve markets on Theseus")]
#[command(version)]
struct Cli {
    /// RPC endpoint URL
    #[arg(long, default_value = "ws://127.0.0.1:9944", global = true)]
    rpc: String,

    /// Seed phrase or secret URI for signing (e.g., "//Alice")
    #[arg(long, default_value = "//Alice", global = true)]
    seed: String,

    /// Contract address (hex, 32 bytes)
    #[arg(long, global = true)]
    contract: Option<String>,

    /// Market Creator agent ID (hex, 32 bytes)
    #[arg(long, global = true)]
    creator_agent: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Create a new prediction market (interactive)
    #[command(alias = "new")]
    CreateMarket {
        /// Market question (if not provided, will prompt interactively)
        #[arg(short, long)]
        question: Option<String>,
    },

    /// Request resolution of a market
    Resolve {
        /// Market ID to resolve
        market_id: u64,
    },

    /// Check market status (dry-run contract call)
    Status {
        /// Market ID to check
        market_id: u64,
    },

    /// Place a bet on a market
    Bet {
        /// Market ID
        market_id: u64,

        /// Option index to bet on (0-based). For binary markets: 0=Yes, 1=No
        #[arg(short, long, default_value = "0")]
        option: u8,

        /// Amount to bet
        amount: u128,
    },

    /// Claim winnings from a resolved market
    Claim {
        /// Market ID
        market_id: u64,
    },

    /// Show configuration
    Config,
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut cli = Cli::parse();

    // Load from environment if not provided via CLI
    if cli.contract.is_none() {
        cli.contract = std::env::var("PM_CONTRACT").ok();
    }
    if cli.creator_agent.is_none() {
        cli.creator_agent = std::env::var("PM_CREATOR_AGENT").ok();
    }

    match &cli.command {
        Commands::CreateMarket { question } => {
            create_market(&cli, question.clone()).await?;
        }
        Commands::Resolve { market_id } => {
            resolve_market(&cli, *market_id).await?;
        }
        Commands::Status { market_id } => {
            check_status(&cli, *market_id).await?;
        }
        Commands::Bet {
            market_id,
            option,
            amount,
        } => {
            place_bet(&cli, *market_id, *option, *amount).await?;
        }
        Commands::Claim { market_id } => {
            claim_winnings(&cli, *market_id).await?;
        }
        Commands::Config => {
            show_config(&cli);
        }
    }

    Ok(())
}

/// Connect to the chain
async fn connect(rpc_url: &str) -> Result<OnlineClient<TheseusConfig>> {
    println!("{} Connecting to {}...", style("[*]").dim(), rpc_url);
    let api = OnlineClient::<TheseusConfig>::from_url(rpc_url)
        .await
        .context("connecting to chain")?;
    println!("{} Connected!", CHECK);
    Ok(api)
}

/// Parse seed into subxt signer
fn parse_signer(seed: &str) -> Result<Keypair> {
    if let Ok(uri) = seed.parse() {
        if let Ok(keypair) = Keypair::from_uri(&uri) {
            return Ok(keypair);
        }
    }

    let seed_str = seed.trim_start_matches("0x");
    if seed_str.len() == 64 {
        let bytes = hex::decode(seed_str).context("decoding hex seed")?;
        let seed_array: [u8; 32] = bytes
            .try_into()
            .map_err(|_| anyhow!("invalid seed length"))?;
        return Ok(Keypair::from_secret_key(seed_array)?);
    }

    anyhow::bail!("Could not parse seed")
}

/// Parse hex string to 32-byte account ID
fn parse_account_id(hex_str: &str) -> Result<[u8; 32]> {
    let hex_str = hex_str.trim_start_matches("0x");
    let bytes = hex::decode(hex_str).context("decoding hex")?;
    bytes
        .try_into()
        .map_err(|_| anyhow!("account ID must be 32 bytes"))
}

/// Interactive market creation - triggers agent run with pause/resume support
async fn create_market(cli: &Cli, question: Option<String>) -> Result<()> {
    let creator_agent = cli
        .creator_agent
        .as_ref()
        .ok_or_else(|| anyhow!("Market Creator agent not set. Use --creator-agent or PM_CREATOR_AGENT"))?;

    println!();
    println!(
        "{}{}",
        CRYSTAL_BALL,
        style("Create New Prediction Market").bold().cyan()
    );
    println!();

    let theme = ColorfulTheme::default();

    // Get market question
    let question = match question {
        Some(q) => q,
        None => {
            println!("{}", style("What type of market?").bold());
            let market_types = vec![
                "Price market (e.g., BTC above $100k)",
                "Event market (e.g., Will X happen by Y date)",
                "Custom (enter your own question)",
            ];

            let selection = Select::with_theme(&theme)
                .items(&market_types)
                .default(0)
                .interact()?;

            match selection {
                0 => {
                    let asset: String = Input::with_theme(&theme)
                        .with_prompt("Asset (e.g., BTC, ETH, SOL)")
                        .default("BTC".to_string())
                        .interact_text()?;

                    let price: String = Input::with_theme(&theme)
                        .with_prompt("Price threshold (USD)")
                        .default("100000".to_string())
                        .interact_text()?;

                    let direction = Select::with_theme(&theme)
                        .with_prompt("Direction")
                        .items(&["above", "below"])
                        .default(0)
                        .interact()?;

                    let timeframe: String = Input::with_theme(&theme)
                        .with_prompt("When? (e.g., 'in 1 hour', 'at noon UTC')")
                        .default("in 1 hour".to_string())
                        .interact_text()?;

                    format!(
                        "Will {} be {} ${} {}?",
                        asset.to_uppercase(),
                        if direction == 0 { "above" } else { "below" },
                        price,
                        timeframe
                    )
                }
                1 => {
                    let event: String = Input::with_theme(&theme)
                        .with_prompt("What event?")
                        .interact_text()?;

                    let deadline: String = Input::with_theme(&theme)
                        .with_prompt("By when?")
                        .interact_text()?;

                    format!("Will {} by {}?", event, deadline)
                }
                _ => Input::with_theme(&theme)
                    .with_prompt("Enter your market question")
                    .interact_text()?,
            }
        }
    };

    println!();
    println!("{} {}", style("Question:").bold(), question);
    println!();

    if !Confirm::with_theme(&theme)
        .with_prompt("Create this market?")
        .default(true)
        .interact()?
    {
        println!("Cancelled.");
        return Ok(());
    }

    // Connect and submit
    let api = connect(&cli.rpc).await?;
    let signer = parse_signer(&cli.seed)?;
    let agent_id = parse_account_id(creator_agent)?;

    println!();
    println!("{} Triggering Market Creator agent...", style("[1/3]").bold());

    // Build the run_agent extrinsic
    let input_bytes = question.as_bytes().to_vec();

    let tx = subxt::dynamic::tx(
        "Agents",
        "run_agent",
        vec![
            Value::from_bytes(&agent_id),
            Value::from_bytes(&input_bytes),
        ],
    );

    let tx_progress = api
        .tx()
        .sign_and_submit_then_watch_default(&tx, &signer)
        .await
        .context("submitting run_agent transaction")?;

    let tx_hash = tx_progress.extrinsic_hash();
    println!("  Transaction: 0x{}", hex::encode(tx_hash.0));

    println!("{} Waiting for agent response...", style("[2/3]").bold());

    let events = tx_progress
        .wait_for_finalized_success()
        .await
        .context("waiting for finalization")?;

    // Check events to find run_id and status
    let mut run_id: Option<u64> = None;
    let mut is_waiting = false;
    let mut is_complete = false;

    for event in events.iter() {
        if let Ok(ev) = event {
            match (ev.pallet_name(), ev.variant_name()) {
                ("Agents", "AgentCallQueued") => {
                    // Try to extract run_id from event (first field is u64)
                    let bytes = ev.field_bytes();
                    if bytes.len() >= 8 {
                        run_id = Some(u64::from_le_bytes(bytes[0..8].try_into().unwrap_or([0; 8])));
                    }
                    println!("  Agent run queued (run_id: {:?})", run_id);
                }
                ("Agents", "AgentRunWaitingForInput") => {
                    is_waiting = true;
                    // Extract run_id if not already set
                    if run_id.is_none() {
                        let bytes = ev.field_bytes();
                        if bytes.len() >= 8 {
                            run_id = Some(u64::from_le_bytes(bytes[0..8].try_into().unwrap_or([0; 8])));
                        }
                    }
                }
                ("Agents", "AgentCallCompleted") => {
                    is_complete = true;
                }
                _ => {}
            }
        }
    }

    // Handle pause/resume loop for clarifications
    if is_waiting {
        if let Some(rid) = run_id {
            println!();
            println!(
                "{}",
                style("Agent needs clarification!").yellow().bold()
            );
            
            // Loop for up to 3 clarifications
            let current_run_id = rid;
            for clarification_num in 1..=3 {
                println!();
                let response: String = Input::with_theme(&theme)
                    .with_prompt(format!("Clarification #{}", clarification_num))
                    .interact_text()?;

                println!();
                println!(
                    "{} Sending clarification...",
                    style(format!("[{}/3]", clarification_num + 1)).bold()
                );

                // Build resume input as JSON: {"response": "user input"}
                let resume_input = format!(r#"{{"response":"{}"}}"#, response.replace('"', "\\\""));
                let resume_bytes = resume_input.as_bytes().to_vec();

                let resume_tx = subxt::dynamic::tx(
                    "Agents",
                    "resume_agent_run",
                    vec![
                        Value::u128(current_run_id as u128),
                        Value::from_bytes(&resume_bytes),
                    ],
                );

                let resume_progress = api
                    .tx()
                    .sign_and_submit_then_watch_default(&resume_tx, &signer)
                    .await
                    .context("submitting resume_agent_run transaction")?;

                let resume_events = resume_progress
                    .wait_for_finalized_success()
                    .await
                    .context("waiting for resume finalization")?;

                // Check if agent completed or needs more input
                let mut still_waiting = false;
                for event in resume_events.iter() {
                    if let Ok(ev) = event {
                        match (ev.pallet_name(), ev.variant_name()) {
                            ("Agents", "AgentRunWaitingForInput") => {
                                still_waiting = true;
                            }
                            ("Agents", "AgentCallCompleted") => {
                                println!();
                                println!("{}Market created successfully!", CHECK);
                                return Ok(());
                            }
                            ("Agents", "AgentCallFailed") => {
                                println!();
                                println!(
                                    "{}Agent failed. Check chain events for details.",
                                    style("Error: ").red()
                                );
                                return Ok(());
                            }
                            _ => {}
                        }
                    }
                }

                if !still_waiting {
                    // Agent completed without explicit event, or continued processing
                    println!();
                    println!("{}Agent processing complete.", CHECK);
                    return Ok(());
                }
            }

            println!();
            println!(
                "{}",
                style("Max clarifications reached. Agent may still be processing.").yellow()
            );
        }
    } else if is_complete {
        println!();
        println!("{}Market created successfully!", CHECK);
    } else {
        println!();
        println!(
            "{}Agent run submitted. Check chain events for status.",
            CHECK
        );
    }

    println!();
    println!("{}", style("The Market Creator agent will:").dim());
    println!("  1. Parse your request");
    println!("  2. Generate structured market parameters");
    println!("  3. Call the contract to create the market");

    Ok(())
}

/// Request market resolution - calls contract
async fn resolve_market(cli: &Cli, market_id: u64) -> Result<()> {
    let contract = cli
        .contract
        .as_ref()
        .ok_or_else(|| anyhow!("Contract address not set. Use --contract or PM_CONTRACT"))?;

    println!();
    println!(
        "{}{}",
        CLOCK,
        style(format!("Requesting Resolution for Market #{}", market_id))
            .bold()
            .cyan()
    );
    println!();

    let theme = ColorfulTheme::default();

    if !Confirm::with_theme(&theme)
        .with_prompt("Request resolution? (This will trigger the Resolver Oracle)")
        .default(true)
        .interact()?
    {
        println!("Cancelled.");
        return Ok(());
    }

    let api = connect(&cli.rpc).await?;
    let signer = parse_signer(&cli.seed)?;
    let contract_addr = parse_account_id(contract)?;

    println!();
    println!("{} Calling contract.request_resolution...", style("[1/2]").bold());

    // Build call data: selector + market_id
    // Selector for request_resolution: 0x03000001
    let mut call_data = vec![0x03, 0x00, 0x00, 0x01];
    call_data.extend_from_slice(&market_id.encode());

    // pallet_contracts::call(dest, value, gas_limit, storage_deposit_limit, data)
    let tx = subxt::dynamic::tx(
        "Contracts",
        "call",
        vec![
            Value::unnamed_variant("Id", [Value::from_bytes(&contract_addr)]),
            Value::u128(0), // value
            Value::unnamed_variant("Limited", [Value::u128(10_000_000_000)]), // gas_limit (Weight as single u64 for ref_time)
            Value::unnamed_variant("None", []), // storage_deposit_limit
            Value::from_bytes(&call_data),
        ],
    );

    let tx_progress = api
        .tx()
        .sign_and_submit_then_watch_default(&tx, &signer)
        .await
        .context("submitting contract call")?;

    let tx_hash = tx_progress.extrinsic_hash();
    println!("  Transaction: 0x{}", hex::encode(tx_hash.0));

    println!("{} Waiting for finalization...", style("[2/2]").bold());

    let _events = tx_progress
        .wait_for_finalized_success()
        .await
        .context("waiting for finalization")?;

    println!();
    println!("{}Resolution requested!", CHECK);
    println!();
    println!("{}", style("The Resolver Oracle agent will:").dim());
    println!("  1. Receive the request via chain extension");
    println!("  2. Fetch price data or research the outcome");
    println!("  3. Submit resolution via callback");

    Ok(())
}

/// Check market status
async fn check_status(cli: &Cli, market_id: u64) -> Result<()> {
    let contract = cli
        .contract
        .as_ref()
        .ok_or_else(|| anyhow!("Contract address not set. Use --contract or PM_CONTRACT"))?;

    println!();
    println!(
        "{}{}",
        CRYSTAL_BALL,
        style(format!("Market #{} Status", market_id)).bold().cyan()
    );
    println!();

    let api = connect(&cli.rpc).await?;
    let contract_addr = parse_account_id(contract)?;

    // Build call data for get_market
    let mut call_data = vec![0x06, 0x00, 0x00, 0x01];
    call_data.extend_from_slice(&market_id.encode());

    // Use dry_run to query without submitting
    // For now, just show that we would query
    println!("Contract: 0x{}", hex::encode(contract_addr));
    println!("Market ID: {}", market_id);
    println!();
    println!(
        "{}",
        style("Note: dry-run queries require additional runtime API setup.").dim()
    );
    println!(
        "{}",
        style("For now, check chain state via polkadot.js or subxt storage queries.").dim()
    );

    let _ = api; // Keep connection alive for future implementation

    Ok(())
}

/// Place a bet on a specific option
async fn place_bet(cli: &Cli, market_id: u64, option_index: u8, amount: u128) -> Result<()> {
    let contract = cli
        .contract
        .as_ref()
        .ok_or_else(|| anyhow!("Contract address not set. Use --contract or PM_CONTRACT"))?;

    println!();
    println!(
        "{}{}",
        MONEY,
        style(format!("Place Bet on Market #{}, Option {}", market_id, option_index))
            .bold()
            .cyan()
    );
    println!();
    println!("  Option: {} {}", style(option_index).bold(), 
        style("(0=first option, 1=second, etc.)").dim());
    println!("  Amount: {}", style(amount).bold());
    println!();

    let theme = ColorfulTheme::default();

    if !Confirm::with_theme(&theme)
        .with_prompt("Confirm bet?")
        .default(true)
        .interact()?
    {
        println!("Cancelled.");
        return Ok(());
    }

    let api = connect(&cli.rpc).await?;
    let signer = parse_signer(&cli.seed)?;
    let contract_addr = parse_account_id(contract)?;

    println!();
    println!("{} Calling contract.place_bet...", style("[1/2]").bold());

    // Build call data: selector + market_id + option_index + amount
    // Selector: 0x02000001
    let mut call_data = vec![0x02, 0x00, 0x00, 0x01];
    call_data.extend_from_slice(&market_id.encode());
    call_data.extend_from_slice(&option_index.encode());
    call_data.extend_from_slice(&amount.encode());

    let tx = subxt::dynamic::tx(
        "Contracts",
        "call",
        vec![
            Value::unnamed_variant("Id", [Value::from_bytes(&contract_addr)]),
            Value::u128(amount), // value - transfer amount for the bet
            Value::unnamed_variant("Limited", [Value::u128(10_000_000_000)]),
            Value::unnamed_variant("None", []),
            Value::from_bytes(&call_data),
        ],
    );

    let tx_progress = api
        .tx()
        .sign_and_submit_then_watch_default(&tx, &signer)
        .await
        .context("submitting contract call")?;

    let tx_hash = tx_progress.extrinsic_hash();
    println!("  Transaction: 0x{}", hex::encode(tx_hash.0));

    println!("{} Waiting for finalization...", style("[2/2]").bold());

    let _events = tx_progress
        .wait_for_finalized_success()
        .await
        .context("waiting for finalization")?;

    println!();
    println!("{}Bet placed!", CHECK);

    Ok(())
}

/// Claim winnings
async fn claim_winnings(cli: &Cli, market_id: u64) -> Result<()> {
    let contract = cli
        .contract
        .as_ref()
        .ok_or_else(|| anyhow!("Contract address not set. Use --contract or PM_CONTRACT"))?;

    println!();
    println!(
        "{}{}",
        MONEY,
        style(format!("Claim Winnings from Market #{}", market_id))
            .bold()
            .cyan()
    );
    println!();

    let api = connect(&cli.rpc).await?;
    let signer = parse_signer(&cli.seed)?;
    let contract_addr = parse_account_id(contract)?;

    println!("{} Calling contract.claim_winnings...", style("[1/2]").bold());

    // Selector: 0x05000001
    let mut call_data = vec![0x05, 0x00, 0x00, 0x01];
    call_data.extend_from_slice(&market_id.encode());

    let tx = subxt::dynamic::tx(
        "Contracts",
        "call",
        vec![
            Value::unnamed_variant("Id", [Value::from_bytes(&contract_addr)]),
            Value::u128(0),
            Value::unnamed_variant("Limited", [Value::u128(10_000_000_000)]),
            Value::unnamed_variant("None", []),
            Value::from_bytes(&call_data),
        ],
    );

    let tx_progress = api
        .tx()
        .sign_and_submit_then_watch_default(&tx, &signer)
        .await
        .context("submitting contract call")?;

    let tx_hash = tx_progress.extrinsic_hash();
    println!("  Transaction: 0x{}", hex::encode(tx_hash.0));

    println!("{} Waiting for finalization...", style("[2/2]").bold());

    let _events = tx_progress
        .wait_for_finalized_success()
        .await
        .context("waiting for finalization")?;

    println!();
    println!("{}Winnings claimed!", CHECK);

    Ok(())
}

/// Show current configuration
fn show_config(cli: &Cli) {
    println!();
    println!("{}", style("Prediction Market CLI Configuration").bold());
    println!();
    println!("  RPC Endpoint:     {}", cli.rpc);
    println!("  Signer:           {}", &cli.seed[..cli.seed.len().min(20)]);
    println!(
        "  Contract:         {}",
        cli.contract.as_deref().unwrap_or("<not set>")
    );
    println!(
        "  Creator Agent:    {}",
        cli.creator_agent.as_deref().unwrap_or("<not set>")
    );
    println!();
    println!("{}", style("Environment Variables:").dim());
    println!("  PM_CONTRACT       - Contract address (hex)");
    println!("  PM_CREATOR_AGENT  - Market Creator agent ID (hex)");
    println!();
    println!("{}", style("Example:").dim());
    println!("  export PM_CONTRACT=0x1234...abcd");
    println!("  export PM_CREATOR_AGENT=0x5678...efgh");
    println!("  pm create-market");
    println!();
}
