# Prediction Market Demo

A multi-option prediction market demonstrating bidirectional agent-contract interactions on Theseus.

**Features:**
- **Multi-option markets**: Supports 2-10 options (binary Yes/No or custom options like Team A/B/Draw)
- **Parimutuel betting**: No odds at bet time; payout proportional to pool
- **Agent → Contract**: Market Creator agent calls the contract to create markets
- **Contract → Agent**: Contract requests the Resolver Oracle agent to resolve markets

## Quick Start with CLI

The `pm` CLI connects directly to the Theseus chain via subxt - no external tools needed.

```bash
# Build the CLI
cd cli && cargo build --release
alias pm='./target/release/pm'

# Set up environment (after deploying contract and registering agents)
export PM_CONTRACT=0x...   # Contract address (32 bytes hex)
export PM_CREATOR_AGENT=0x...  # Market Creator agent ID

# Create a market (interactive wizard)
pm create-market

# Or with a direct question
pm create-market -q "Will BTC be above \$100k at noon UTC?"

# Place bets (option index: 0=first, 1=second, etc.)
pm bet 0 --option 0 1000    # Bet on first option (e.g., "Yes")
pm bet 0 --option 1 1000    # Bet on second option (e.g., "No")
pm bet 0 -o 2 500           # Bet on third option (for multi-option markets)

# Check status
pm status 0

# Request resolution (after deadline - triggers Resolver Oracle)
pm resolve 0

# Claim winnings (after resolution)
pm claim 0

# Show config
pm config
```

**Options:**
- `--rpc <URL>` - Chain RPC endpoint (default: `ws://127.0.0.1:9944`)
- `--seed <SEED>` - Signer seed/URI (default: `//Alice`)
- `--contract <HEX>` - Contract address (or `PM_CONTRACT` env var)
- `--creator-agent <HEX>` - Agent ID (or `PM_CREATOR_AGENT` env var)

## Architecture

```
┌─────────────────┐                      ┌─────────────────────────┐
│                 │  1. Natural language │                         │
│     User/EOA    │─────prompt──────────>│  Market Creator Agent   │
│                 │                      │  (market_creator.ship)  │
└────────┬────────┘                      └───────────┬─────────────┘
         │                                           │
         │ 3. place_bet()                            │ 2. contracts.call(create_market)
         │                                           ▼
         │                               ┌─────────────────────────┐
         └──────────────────────────────>│   Prediction Market     │
                                         │      Contract           │
         ┌──────────────────────────────>│   (prediction_market)   │
         │ 7. claim_winnings()           └───────────┬─────────────┘
         │                                           │
┌────────┴────────┐                                  │ 4. chain_ext(agents_request)
│                 │                                  ▼
│     User/EOA    │                      ┌─────────────────────────┐
│                 │                      │  Resolver Oracle Agent  │
└─────────────────┘                      │  (resolver_oracle.ship) │
                                         │                         │
                                         │  5. get_price/web_search│
                                         └───────────┬─────────────┘
                                                     │
                                                     │ 6. callback(resolution)
                                                     ▼
                                         ┌─────────────────────────┐
                                         │   Prediction Market     │
                                         │      Contract           │
                                         │   (prediction_market)   │
                                         └─────────────────────────┘
```

## Components

### 1. Prediction Market Contract (`contract/`)

A Rust smart contract that manages:
- Market creation (restricted to Market Creator agent)
- Bet placement (YES/NO shares)
- Resolution requests (via chain extension to Resolver Oracle)
- Settlement and payout distribution

### 2. Market Creator Agent (`agents/market_creator.ship`)

A SHIP agent that:
- Takes natural language market requests from users
- Asks clarifying questions if the market is ambiguous
- Generates structured market parameters
- Calls the contract's `create_market` function

### 3. Resolver Oracle Agent (`agents/resolver_oracle.ship`)

A SHIP agent that:
- Only accepts requests from the contract (via chain extension)
- Uses `get_price` tool for price-based markets
- Uses `web_search`/`fetch_url` for event-based markets
- Returns structured resolution via callback

### 4. CLI (`cli/`)

A user-friendly CLI for interacting with the prediction market:

```
pm create-market    Create a new market (interactive wizard)
pm resolve <id>     Request resolution of a market
pm status <id>      Check market status
pm bet <id> <amt>   Place a bet (--yes for YES, default NO)
pm claim <id>       Claim winnings
pm config           Show current configuration
```

### 5. Price Tool (`get_price`)

Added to the tool-executor, fetches cryptocurrency prices from CoinGecko:
- Supports common symbols (BTC, ETH, SOL, etc.)
- Free tier API, no key required
- Rate-limited to avoid throttling

## Quick Start

### Prerequisites

- Theseus node running locally (`ws://127.0.0.1:9944`)
- Rust with `wasm32-unknown-unknown` target
- `theseus-cli` installed

### Setup

```bash
# Run the setup script
./scripts/setup_demo.sh
```

The script will guide you through:
1. Building the contract
2. Deploying to the chain
3. Registering both agents
4. Configuring the contract with agent addresses

### Demo Flow

**1. Create a Market**

```bash
theseus-cli agent run <CREATOR_ID> \
  --input "Create a market for whether Bitcoin will be above $100,000 at noon UTC today"
```

The Market Creator will:
- Parse your request
- Ask clarifying questions if needed
- Create the market on-chain

**2. Place Bets**

```bash
# Bet YES (1000 units)
theseus-cli contract call <CONTRACT> place_bet 0 true 1000

# Bet NO (1000 units)
theseus-cli contract call <CONTRACT> place_bet 0 false 1000
```

**3. Request Resolution** (after deadline)

```bash
theseus-cli contract call <CONTRACT> request_resolution 0
```

This triggers the Resolver Oracle agent via chain extension.

**4. Claim Winnings** (after resolution)

```bash
theseus-cli contract call <CONTRACT> claim_winnings 0
```

## Example Markets

### Binary Markets (Yes/No)

Simple two-option markets:

- "Will BTC be above $100k at 12:00 UTC?" → `["Yes", "No"]`
- "Will ETH be below $3,000 in 10 minutes?" → `["Yes", "No"]`
- "Higher or lower than current price?" → `["Higher", "Lower"]`

### Multi-Option Markets

Markets with 3+ outcomes:

- "Who wins the championship?" → `["Team A", "Team B", "Team C", "Draw"]`
- "What will BTC price be?" → `["Below $50k", "$50k-$75k", "$75k-$100k", "Above $100k"]`
- "Which candidate wins?" → `["Candidate A", "Candidate B", "Candidate C"]`

### Quick Resolution (for demos)

Resolve in minutes using `get_price` tool:

- Price threshold markets
- Time-bound price predictions

### Long-Term (for testnet)

Resolve over days/weeks using `web_search`:

- Event outcomes
- Sports results
- Political events

## Contract Functions

| Function | Selector | Description |
|----------|----------|-------------|
| `set_market_creator` | `0x00000002` | Admin: Set market creator agent |
| `set_resolver_oracle` | `0x00000003` | Admin: Set resolver oracle agent |
| `create_market` | `0x01000001` | Create market with options array |
| `place_bet` | `0x02000001` | Bet on option by index |
| `request_resolution` | `0x03000001` | Request market resolution |
| `claim_winnings` | `0x05000001` | Claim winnings after resolution |
| `get_market` | `0x06000001` | View market details |
| `get_position` | `0x07000001` | View user position |
| `get_implied_odds` | `0x08000001` | View current implied odds |

## Troubleshooting

### "Market creator agent not configured"

The contract hasn't been configured with agent addresses. Run:
```bash
theseus-cli contract call <CONTRACT> set_market_creator <CREATOR_ID>
```

### "Only market creator agent can create markets"

You're trying to create a market directly instead of through the agent. Use:
```bash
theseus-cli agent run <CREATOR_ID> --input "your market request"
```

### "Resolver only accepts contract requests"

The Resolver Oracle agent rejected a direct call. It only accepts requests from the contract via chain extension. This is a security feature.

### "Resolution deadline not reached"

The market's deadline block hasn't passed yet. Wait until the deadline, then call `request_resolution`.

### Price tool returns "Asset not found"

The asset name might not be recognized. Try using:
- Full names: `bitcoin`, `ethereum`, `solana`
- Common symbols: `btc`, `eth`, `sol`
- CoinGecko IDs: `matic-network`, `avalanche-2`

## Development

### Building the Contract

```bash
cd contract
cargo build --release --target wasm32-unknown-unknown
```

### Running Tests

```bash
cd contract
cargo test
```

### Updating Agents

After modifying `.ship` files, re-register the agents:
```bash
theseus-cli agent update <AGENT_ID> agents/market_creator.ship
theseus-cli agent update <AGENT_ID> agents/resolver_oracle.ship
```

## License

Part of the Theseus project - see repository root for license.
