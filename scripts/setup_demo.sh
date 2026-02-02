#!/bin/bash
# Prediction Market Demo Setup Script
#
# Deployment order:
#   1. Register Market Creator agent
#   2. Deploy contract + set market creator
#   3. Register Resolver Oracle + set it
#
# Prerequisites:
#   - theseus-node running locally (ws://127.0.0.1:9944)
#   - Rust toolchain with wasm32-unknown-unknown target
#   - polkadot.js apps for contract deployment and agent registration

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}╔════════════════════════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║         Prediction Market Demo Setup                       ║${NC}"
echo -e "${BLUE}╚════════════════════════════════════════════════════════════╝${NC}"
echo ""

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
CONTRACT_DIR="$PROJECT_DIR/contract"
CLI_DIR="$PROJECT_DIR/cli"
AGENTS_DIR="$PROJECT_DIR/agents"

# Check prerequisites
echo -e "${YELLOW}[0/6]${NC} Checking prerequisites..."

if ! rustup target list --installed | grep -q "wasm32-unknown-unknown"; then
    echo -e "${YELLOW}Installing wasm32-unknown-unknown target...${NC}"
    rustup target add wasm32-unknown-unknown
fi

echo -e "${GREEN}✓${NC} Prerequisites OK"
echo ""

# Step 1: Build the PM CLI
echo -e "${YELLOW}[1/6]${NC} Building prediction market CLI..."
cd "$CLI_DIR"
cargo build --release 2>&1 | tail -3

PM_CLI="$CLI_DIR/target/release/pm"
if [ ! -f "$PM_CLI" ]; then
    echo -e "${RED}Error: CLI not built${NC}"
    exit 1
fi

echo -e "${GREEN}✓${NC} CLI built: $PM_CLI"
echo ""

# Step 2: Build the contract
echo -e "${YELLOW}[2/6]${NC} Building prediction market contract..."
cd "$CONTRACT_DIR"
cargo build --release --target wasm32-unknown-unknown 2>&1 | tail -3

CONTRACT_WASM="$CONTRACT_DIR/target/wasm32-unknown-unknown/release/prediction_market.wasm"

if [ ! -f "$CONTRACT_WASM" ]; then
    echo -e "${RED}Error: Contract WASM not found${NC}"
    exit 1
fi

echo -e "${GREEN}✓${NC} Contract built: $(ls -lh "$CONTRACT_WASM" | awk '{print $5}')"
echo ""

# Step 3: Instructions for registration
echo -e "${YELLOW}[3/6]${NC} Register Market Creator agent"
echo ""
echo -e "${BLUE}Using polkadot.js apps (https://polkadot.js.org/apps):${NC}"
echo ""
echo "  1. Go to Developer > Extrinsics"
echo "  2. Select 'agents' pallet, 'registerAgent' extrinsic"
echo "  3. Upload compiled agent: $AGENTS_DIR/market_creator.ship"
echo "  4. Submit and note the agent account ID from events"
echo ""
echo "  Then set: export PM_CREATOR_AGENT=0x<agent_id>"
echo ""

# Step 4: Deploy contract
echo -e "${YELLOW}[4/6]${NC} Deploy contract and configure"
echo ""
echo -e "${BLUE}Using polkadot.js apps:${NC}"
echo ""
echo "  1. Go to Developer > Contracts > Upload & deploy code"
echo "  2. Upload: $CONTRACT_WASM"
echo "  3. Call constructor with your admin account"
echo "  4. Note the contract address"
echo ""
echo "  Then set: export PM_CONTRACT=0x<contract_address>"
echo ""
echo "  5. Call 'set_market_creator' with \$PM_CREATOR_AGENT"
echo ""

# Step 5: Register resolver
echo -e "${YELLOW}[5/6]${NC} Register Resolver Oracle agent"
echo ""
echo -e "${BLUE}Using polkadot.js apps:${NC}"
echo ""
echo "  1. Go to Developer > Extrinsics"
echo "  2. Select 'agents' pallet, 'registerAgent' extrinsic"
echo "  3. Upload compiled agent: $AGENTS_DIR/resolver_oracle.ship"
echo "  4. Submit and note the agent account ID"
echo ""
echo "  5. Call contract 'set_resolver_oracle' with the resolver agent ID"
echo ""

# Summary
echo -e "${YELLOW}[6/6]${NC} Setup Summary"
echo ""
echo -e "${GREEN}╔════════════════════════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║                    Build Complete!                          ║${NC}"
echo -e "${GREEN}╚════════════════════════════════════════════════════════════╝${NC}"
echo ""
echo "Built artifacts:"
echo "  CLI:      $PM_CLI"
echo "  Contract: $CONTRACT_WASM"
echo "  Agents:   $AGENTS_DIR/*.ship"
echo ""
echo -e "${YELLOW}After registration, set environment:${NC}"
echo ""
echo "  export PM_CONTRACT=0x<contract_address>"
echo "  export PM_CREATOR_AGENT=0x<creator_agent_id>"
echo ""
echo -e "${YELLOW}Then use the CLI:${NC}"
echo ""
echo "  # Create a market (interactive)"
echo "  $PM_CLI create-market"
echo ""
echo "  # Or with a question"
echo "  $PM_CLI create-market -q \"Will BTC be above \\\$100k at noon UTC?\""
echo ""
echo "  # Place bets, resolve, claim"
echo "  $PM_CLI bet 0 --yes 1000"
echo "  $PM_CLI resolve 0"
echo "  $PM_CLI claim 0"
echo ""
echo -e "${GREEN}Happy predicting!${NC}"
