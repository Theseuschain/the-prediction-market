//! Prediction Market Smart Contract
//!
//! A multi-option prediction market that integrates with Theseus agents:
//! - Market Creator Agent: Creates structured markets from user prompts
//! - Resolver Oracle Agent: Resolves markets using external data sources
//!
//! # Pricing Model
//! Uses parimutuel betting - no odds at bet time, payout is proportional to pool:
//!   Payout = (user_shares / winning_option_shares) * total_pool
//!
//! # Flow
//! 1. Admin deploys contract, sets agent addresses
//! 2. Market Creator agent calls `create_market` with options
//! 3. Users place bets via `place_bet(market_id, option_index, amount)`
//! 4. After deadline, anyone calls `request_resolution`
//! 5. Contract requests Resolver Oracle via chain extension
//! 6. Resolver completes, callback triggers `on_resolution_complete`
//! 7. Winners claim via `claim_winnings`

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use alloc::string::String;
use alloc::vec;
use alloc::vec::Vec;
use parity_scale_codec::{Decode, Encode};
use scale_info::TypeInfo;

// ============================================================================
// Types
// ============================================================================

/// Account identifier (32 bytes for Substrate)
pub type AccountId = [u8; 32];

/// Block number
pub type BlockNumber = u64;

/// Balance type
pub type Balance = u128;

/// Market identifier
pub type MarketId = u64;

/// Option index within a market
pub type OptionIndex = u8;

/// Maximum number of options per market
pub const MAX_OPTIONS: usize = 10;

/// Status of a prediction market
#[derive(Clone, Copy, PartialEq, Eq, Encode, Decode, TypeInfo, Debug)]
pub enum MarketStatus {
    /// Market is open for betting
    Open,
    /// Resolution has been requested, waiting for oracle
    PendingResolution,
    /// Market has been resolved
    Resolved,
}

impl Default for MarketStatus {
    fn default() -> Self {
        MarketStatus::Open
    }
}

/// A prediction market with multiple options
#[derive(Clone, Encode, Decode, TypeInfo, Debug)]
pub struct Market {
    /// Unique market identifier
    pub id: MarketId,
    /// The question being predicted
    pub question: String,
    /// Available options (e.g., ["Yes", "No"] or ["Team A", "Team B", "Draw"])
    pub options: Vec<String>,
    /// Criteria for resolution
    pub resolution_criteria: String,
    /// Where to verify the outcome (URL, API, etc.)
    pub resolution_source: String,
    /// Who created this market (should be market creator agent)
    pub creator: AccountId,
    /// Block number after which resolution can be requested
    pub resolution_deadline: BlockNumber,
    /// Total shares per option (indexed by option_index)
    pub shares_per_option: Vec<Balance>,
    /// Current status
    pub status: MarketStatus,
    /// Winning option index (None = unresolved)
    pub winning_option: Option<OptionIndex>,
}

impl Market {
    /// Total pool across all options
    pub fn total_pool(&self) -> Balance {
        self.shares_per_option.iter().sum()
    }

    /// Check if this is a binary (Yes/No) market
    pub fn is_binary(&self) -> bool {
        self.options.len() == 2
    }
}

/// A user's position in a market (shares per option)
#[derive(Clone, Default, Encode, Decode, TypeInfo, Debug)]
pub struct Position {
    /// Shares held for each option (indexed by option_index)
    pub shares: Vec<Balance>,
}

impl Position {
    /// Create a new position with the given number of options
    pub fn new(num_options: usize) -> Self {
        Self {
            shares: vec![0; num_options],
        }
    }

    /// Total shares across all options
    pub fn total_shares(&self) -> Balance {
        self.shares.iter().sum()
    }

    /// Check if position is empty
    pub fn is_empty(&self) -> bool {
        self.shares.iter().all(|&s| s == 0)
    }
}

/// Contract configuration
#[derive(Clone, Default, Encode, Decode, TypeInfo, Debug)]
pub struct Config {
    /// Admin who can update configuration
    pub admin: AccountId,
    /// Market creator agent (only this account can create markets)
    pub market_creator_agent: Option<AccountId>,
    /// Resolver oracle agent (called to resolve markets)
    pub resolver_oracle_agent: Option<AccountId>,
}

// ============================================================================
// Chain Extension Types (for calling agents)
// ============================================================================

/// Request sent to the resolver oracle agent
#[derive(Clone, Encode, Decode, TypeInfo, Debug)]
pub struct MarketResolutionRequest {
    pub market_id: MarketId,
    pub question: String,
    pub options: Vec<String>,
    pub resolution_criteria: String,
    pub resolution_source: String,
}

/// Callback specification for agent response
#[derive(Clone, Encode, Decode, TypeInfo, Debug)]
pub struct CallbackSpec {
    /// Selector for callback function
    pub selector: [u8; 4],
    /// Gas budget for callback execution
    pub gas_limit: u64,
}

/// Full request to chain extension
#[derive(Clone, Encode, Decode, TypeInfo, Debug)]
pub struct ContractAgentRequest {
    /// Target agent account ID
    pub target_agent: AccountId,
    /// SCALE-encoded input for the agent
    pub input: Vec<u8>,
    /// Time-to-live in blocks
    pub ttl_blocks: u32,
    /// Optional callback when agent completes
    pub callback: Option<CallbackSpec>,
}

/// Receipt returned from chain extension
#[derive(Clone, Encode, Decode, TypeInfo, Debug)]
pub struct AgentRunReceipt {
    pub request_id: u64,
    pub estimated_start_block: BlockNumber,
}

/// Payload delivered in callback
#[derive(Clone, Encode, Decode, TypeInfo, Debug)]
pub struct AgentCallbackPayload {
    pub request_id: u64,
    pub run_id: u64,
    pub success: bool,
    pub output: Vec<u8>,
}

/// Resolution result from oracle agent
#[derive(Clone, Encode, Decode, TypeInfo, Debug)]
pub struct ResolutionResult {
    pub market_id: MarketId,
    /// Index of the winning option (0-based)
    pub winning_option: OptionIndex,
    /// Confidence percentage (0-100)
    pub confidence_pct: u8,
    /// Summary of evidence used
    pub evidence_summary: String,
}

// ============================================================================
// Contract Storage
// ============================================================================

/// Contract storage layout
#[derive(Default)]
pub struct PredictionMarket {
    /// Contract configuration
    pub config: Config,
    /// Next market ID to assign
    pub next_market_id: MarketId,
    /// All markets by ID
    pub markets: Vec<(MarketId, Market)>,
    /// User positions: (market_id, account) -> Position
    pub positions: Vec<((MarketId, AccountId), Position)>,
    /// Pending resolution requests: market_id -> request_id
    pub pending_resolutions: Vec<(MarketId, u64)>,
}

// ============================================================================
// Contract Implementation
// ============================================================================

impl PredictionMarket {
    // ------------------------------------------------------------------------
    // Constructor
    // ------------------------------------------------------------------------

    /// Initialize the contract with an admin
    pub fn new(admin: AccountId) -> Self {
        Self {
            config: Config {
                admin,
                market_creator_agent: None,
                resolver_oracle_agent: None,
            },
            next_market_id: 0,
            markets: Vec::new(),
            positions: Vec::new(),
            pending_resolutions: Vec::new(),
        }
    }

    // ------------------------------------------------------------------------
    // Admin Functions
    // ------------------------------------------------------------------------

    /// Set the market creator agent address (admin only)
    pub fn set_market_creator(&mut self, caller: AccountId, agent_id: AccountId) -> Result<(), &'static str> {
        if caller != self.config.admin {
            return Err("Only admin can set market creator");
        }
        self.config.market_creator_agent = Some(agent_id);
        Ok(())
    }

    /// Set the resolver oracle agent address (admin only)
    pub fn set_resolver_oracle(&mut self, caller: AccountId, agent_id: AccountId) -> Result<(), &'static str> {
        if caller != self.config.admin {
            return Err("Only admin can set resolver oracle");
        }
        self.config.resolver_oracle_agent = Some(agent_id);
        Ok(())
    }

    // ------------------------------------------------------------------------
    // Market Lifecycle
    // ------------------------------------------------------------------------

    /// Create a new prediction market (Market Creator Agent only)
    /// 
    /// For binary markets, use options = ["Yes", "No"]
    pub fn create_market(
        &mut self,
        caller: AccountId,
        question: String,
        options: Vec<String>,
        resolution_criteria: String,
        resolution_source: String,
        resolution_deadline: BlockNumber,
    ) -> Result<MarketId, &'static str> {
        // Access control: only market creator agent
        let creator_agent = self.config.market_creator_agent
            .ok_or("Market creator agent not configured")?;
        
        if caller != creator_agent {
            return Err("Only market creator agent can create markets");
        }

        // Validate options
        if options.len() < 2 {
            return Err("Market must have at least 2 options");
        }
        if options.len() > MAX_OPTIONS {
            return Err("Too many options");
        }

        let market_id = self.next_market_id;
        self.next_market_id += 1;

        let num_options = options.len();
        let market = Market {
            id: market_id,
            question,
            options,
            resolution_criteria,
            resolution_source,
            creator: caller,
            resolution_deadline,
            shares_per_option: vec![0; num_options],
            status: MarketStatus::Open,
            winning_option: None,
        };

        self.markets.push((market_id, market));
        Ok(market_id)
    }

    /// Place a bet on a specific option
    pub fn place_bet(
        &mut self,
        caller: AccountId,
        market_id: MarketId,
        option_index: OptionIndex,
        amount: Balance,
    ) -> Result<(), &'static str> {
        // Find market
        let market = self.markets.iter_mut()
            .find(|(id, _)| *id == market_id)
            .map(|(_, m)| m)
            .ok_or("Market not found")?;

        // Check market is open
        if market.status != MarketStatus::Open {
            return Err("Market is not open for betting");
        }

        // Validate option index
        let idx = option_index as usize;
        if idx >= market.options.len() {
            return Err("Invalid option index");
        }

        // Update market totals
        market.shares_per_option[idx] += amount;

        // Update user position
        let key = (market_id, caller);
        let position = self.positions.iter_mut()
            .find(|(k, _)| *k == key)
            .map(|(_, p)| p);

        match position {
            Some(pos) => {
                // Ensure position has right size
                while pos.shares.len() < market.options.len() {
                    pos.shares.push(0);
                }
                pos.shares[idx] += amount;
            }
            None => {
                let mut new_pos = Position::new(market.options.len());
                new_pos.shares[idx] = amount;
                self.positions.push((key, new_pos));
            }
        }

        Ok(())
    }

    /// Request market resolution (anyone can call after deadline)
    /// Returns the agent request to be sent via chain extension
    pub fn request_resolution(
        &mut self,
        market_id: MarketId,
        current_block: BlockNumber,
    ) -> Result<ContractAgentRequest, &'static str> {
        // Find market
        let market = self.markets.iter_mut()
            .find(|(id, _)| *id == market_id)
            .map(|(_, m)| m)
            .ok_or("Market not found")?;

        // Check deadline passed
        if current_block < market.resolution_deadline {
            return Err("Resolution deadline not reached");
        }

        // Check not already pending/resolved
        if market.status != MarketStatus::Open {
            return Err("Market is not open");
        }

        // Get resolver agent
        let resolver = self.config.resolver_oracle_agent
            .ok_or("Resolver oracle not configured")?;

        // Update status
        market.status = MarketStatus::PendingResolution;

        // Build resolution request
        let request_input = MarketResolutionRequest {
            market_id,
            question: market.question.clone(),
            options: market.options.clone(),
            resolution_criteria: market.resolution_criteria.clone(),
            resolution_source: market.resolution_source.clone(),
        };

        // Build chain extension request
        Ok(ContractAgentRequest {
            target_agent: resolver,
            input: request_input.encode(),
            ttl_blocks: 100, // ~10 minutes at 6s blocks
            callback: Some(CallbackSpec {
                selector: [0x04, 0x00, 0x00, 0x01], // on_resolution_complete
                gas_limit: 1_000_000_000, // 1B gas for settlement
            }),
        })
    }

    /// Handle resolution callback from oracle agent
    pub fn on_resolution_complete(
        &mut self,
        callback_payload: AgentCallbackPayload,
    ) -> Result<(), &'static str> {
        if !callback_payload.success {
            // Agent failed - could implement retry logic here
            return Err("Oracle agent failed to resolve");
        }

        // Decode resolution result
        let result = ResolutionResult::decode(&mut &callback_payload.output[..])
            .map_err(|_| "Failed to decode resolution result")?;

        // Find market
        let market = self.markets.iter_mut()
            .find(|(id, _)| *id == result.market_id)
            .map(|(_, m)| m)
            .ok_or("Market not found")?;

        // Verify market is pending
        if market.status != MarketStatus::PendingResolution {
            return Err("Market is not pending resolution");
        }

        // Validate winning option
        if result.winning_option as usize >= market.options.len() {
            return Err("Invalid winning option index");
        }

        // Apply resolution
        market.status = MarketStatus::Resolved;
        market.winning_option = Some(result.winning_option);

        // Remove from pending
        self.pending_resolutions.retain(|(id, _)| *id != result.market_id);

        Ok(())
    }

    /// Claim winnings from a resolved market
    pub fn claim_winnings(
        &mut self,
        caller: AccountId,
        market_id: MarketId,
    ) -> Result<Balance, &'static str> {
        // Find market
        let market = self.markets.iter()
            .find(|(id, _)| *id == market_id)
            .map(|(_, m)| m)
            .ok_or("Market not found")?;

        // Check resolved
        if market.status != MarketStatus::Resolved {
            return Err("Market not resolved");
        }

        let winning_idx = market.winning_option.ok_or("No winning option set")? as usize;

        // Find user position
        let key = (market_id, caller);
        let position_idx = self.positions.iter()
            .position(|(k, _)| *k == key)
            .ok_or("No position in this market")?;

        let (_, position) = &self.positions[position_idx];

        // Check user has shares in winning option
        if winning_idx >= position.shares.len() {
            return Err("No winning shares");
        }

        let winning_shares = position.shares[winning_idx];
        if winning_shares == 0 {
            return Err("No winning shares");
        }

        // Calculate payout: winner gets proportional share of total pool
        // Payout = (user_shares / winning_pool) * total_pool
        let total_pool = market.total_pool();
        let winning_pool = market.shares_per_option[winning_idx];

        if winning_pool == 0 {
            return Err("No shares in winning option");
        }

        let payout = (winning_shares as u128 * total_pool as u128) / winning_pool as u128;

        // Remove position (claimed)
        self.positions.remove(position_idx);

        Ok(payout as Balance)
    }

    // ------------------------------------------------------------------------
    // View Functions
    // ------------------------------------------------------------------------

    /// Get market details
    pub fn get_market(&self, market_id: MarketId) -> Option<&Market> {
        self.markets.iter()
            .find(|(id, _)| *id == market_id)
            .map(|(_, m)| m)
    }

    /// Get user position in a market
    pub fn get_position(&self, market_id: MarketId, account: AccountId) -> Position {
        let key = (market_id, account);
        self.positions.iter()
            .find(|(k, _)| *k == key)
            .map(|(_, p)| p.clone())
            .unwrap_or_default()
    }

    /// Get contract configuration
    pub fn get_config(&self) -> &Config {
        &self.config
    }

    /// Get implied odds for each option (based on current shares)
    /// Returns percentages that sum to 100
    pub fn get_implied_odds(&self, market_id: MarketId) -> Option<Vec<u8>> {
        let market = self.get_market(market_id)?;
        let total = market.total_pool();
        
        if total == 0 {
            // Equal odds when no bets
            let equal = 100 / market.options.len() as u8;
            return Some(vec![equal; market.options.len()]);
        }

        Some(
            market.shares_per_option
                .iter()
                .map(|&shares| ((shares * 100) / total) as u8)
                .collect()
        )
    }
}

// ============================================================================
// Entry Point Dispatch (WASM)
// ============================================================================

/// Function selectors
pub mod selectors {
    pub const CONSTRUCTOR: [u8; 4] = [0x00, 0x00, 0x00, 0x01];
    pub const SET_MARKET_CREATOR: [u8; 4] = [0x00, 0x00, 0x00, 0x02];
    pub const SET_RESOLVER_ORACLE: [u8; 4] = [0x00, 0x00, 0x00, 0x03];
    pub const CREATE_MARKET: [u8; 4] = [0x01, 0x00, 0x00, 0x01];
    pub const PLACE_BET: [u8; 4] = [0x02, 0x00, 0x00, 0x01];
    pub const REQUEST_RESOLUTION: [u8; 4] = [0x03, 0x00, 0x00, 0x01];
    pub const ON_RESOLUTION_COMPLETE: [u8; 4] = [0x04, 0x00, 0x00, 0x01];
    pub const CLAIM_WINNINGS: [u8; 4] = [0x05, 0x00, 0x00, 0x01];
    pub const GET_MARKET: [u8; 4] = [0x06, 0x00, 0x00, 0x01];
    pub const GET_POSITION: [u8; 4] = [0x07, 0x00, 0x00, 0x01];
    pub const GET_IMPLIED_ODDS: [u8; 4] = [0x08, 0x00, 0x00, 0x01];
}

#[cfg(test)]
mod tests {
    use super::*;

    fn alice() -> AccountId {
        [1u8; 32]
    }

    fn bob() -> AccountId {
        [2u8; 32]
    }

    fn charlie() -> AccountId {
        [3u8; 32]
    }

    fn market_creator() -> AccountId {
        [10u8; 32]
    }

    fn resolver_oracle() -> AccountId {
        [11u8; 32]
    }

    #[test]
    fn test_constructor() {
        let contract = PredictionMarket::new(alice());
        assert_eq!(contract.config.admin, alice());
        assert!(contract.config.market_creator_agent.is_none());
        assert!(contract.config.resolver_oracle_agent.is_none());
    }

    #[test]
    fn test_set_agents() {
        let mut contract = PredictionMarket::new(alice());
        
        // Admin can set agents
        assert!(contract.set_market_creator(alice(), market_creator()).is_ok());
        assert!(contract.set_resolver_oracle(alice(), resolver_oracle()).is_ok());
        
        assert_eq!(contract.config.market_creator_agent, Some(market_creator()));
        assert_eq!(contract.config.resolver_oracle_agent, Some(resolver_oracle()));
        
        // Non-admin cannot
        assert!(contract.set_market_creator(bob(), bob()).is_err());
    }

    #[test]
    fn test_create_binary_market() {
        let mut contract = PredictionMarket::new(alice());
        contract.set_market_creator(alice(), market_creator()).unwrap();
        
        // Binary market with Yes/No
        let market_id = contract.create_market(
            market_creator(),
            "Will BTC hit 100k?".into(),
            vec!["Yes".into(), "No".into()],
            "Price >= $100,000 on CoinGecko".into(),
            "https://coingecko.com".into(),
            100,
        ).unwrap();
        
        assert_eq!(market_id, 0);
        
        let market = contract.get_market(0).unwrap();
        assert_eq!(market.question, "Will BTC hit 100k?");
        assert_eq!(market.options.len(), 2);
        assert!(market.is_binary());
        assert_eq!(market.status, MarketStatus::Open);
    }

    #[test]
    fn test_create_multi_option_market() {
        let mut contract = PredictionMarket::new(alice());
        contract.set_market_creator(alice(), market_creator()).unwrap();
        
        // Multi-option market
        let market_id = contract.create_market(
            market_creator(),
            "Who will win the championship?".into(),
            vec!["Team A".into(), "Team B".into(), "Team C".into(), "Draw".into()],
            "Official tournament results".into(),
            "https://tournament.com".into(),
            1000,
        ).unwrap();
        
        let market = contract.get_market(market_id).unwrap();
        assert_eq!(market.options.len(), 4);
        assert!(!market.is_binary());
    }

    #[test]
    fn test_place_bet_multi_option() {
        let mut contract = PredictionMarket::new(alice());
        contract.set_market_creator(alice(), market_creator()).unwrap();
        
        contract.create_market(
            market_creator(),
            "Test?".into(),
            vec!["A".into(), "B".into(), "C".into()],
            "Criteria".into(),
            "Source".into(),
            100,
        ).unwrap();
        
        // Place bets on different options
        contract.place_bet(alice(), 0, 0, 100).unwrap(); // Alice bets on A
        contract.place_bet(bob(), 0, 1, 200).unwrap();   // Bob bets on B
        contract.place_bet(charlie(), 0, 2, 150).unwrap(); // Charlie bets on C
        
        let market = contract.get_market(0).unwrap();
        assert_eq!(market.shares_per_option[0], 100);
        assert_eq!(market.shares_per_option[1], 200);
        assert_eq!(market.shares_per_option[2], 150);
        assert_eq!(market.total_pool(), 450);
        
        let alice_pos = contract.get_position(0, alice());
        assert_eq!(alice_pos.shares, vec![100, 0, 0]);
    }

    #[test]
    fn test_implied_odds() {
        let mut contract = PredictionMarket::new(alice());
        contract.set_market_creator(alice(), market_creator()).unwrap();
        
        contract.create_market(
            market_creator(),
            "Test?".into(),
            vec!["A".into(), "B".into()],
            "Criteria".into(),
            "Source".into(),
            100,
        ).unwrap();
        
        // No bets - equal odds
        let odds = contract.get_implied_odds(0).unwrap();
        assert_eq!(odds, vec![50, 50]);
        
        // After bets: 75% on A, 25% on B
        contract.place_bet(alice(), 0, 0, 300).unwrap();
        contract.place_bet(bob(), 0, 1, 100).unwrap();
        
        let odds = contract.get_implied_odds(0).unwrap();
        assert_eq!(odds, vec![75, 25]);
    }

    #[test]
    fn test_full_lifecycle_multi_option() {
        let mut contract = PredictionMarket::new(alice());
        contract.set_market_creator(alice(), market_creator()).unwrap();
        contract.set_resolver_oracle(alice(), resolver_oracle()).unwrap();
        
        // Create market with 3 options
        let market_id = contract.create_market(
            market_creator(),
            "Which team wins?".into(),
            vec!["Team A".into(), "Team B".into(), "Draw".into()],
            "Official results".into(),
            "tournament.com".into(),
            100,
        ).unwrap();
        
        // Place bets
        contract.place_bet(alice(), market_id, 0, 100).unwrap();   // Team A
        contract.place_bet(bob(), market_id, 1, 100).unwrap();     // Team B
        contract.place_bet(charlie(), market_id, 2, 100).unwrap(); // Draw
        
        // Request resolution (after deadline)
        let request = contract.request_resolution(market_id, 101).unwrap();
        assert_eq!(request.target_agent, resolver_oracle());
        
        // Simulate callback (Team B wins - option index 1)
        let result = ResolutionResult {
            market_id,
            winning_option: 1, // Team B
            confidence_pct: 95,
            evidence_summary: "Team B won 3-1".into(),
        };
        
        let callback = AgentCallbackPayload {
            request_id: 1,
            run_id: 1,
            success: true,
            output: result.encode(),
        };
        
        contract.on_resolution_complete(callback).unwrap();
        
        let market = contract.get_market(market_id).unwrap();
        assert_eq!(market.status, MarketStatus::Resolved);
        assert_eq!(market.winning_option, Some(1));
        
        // Bob (Team B) wins - gets entire pool
        let bob_payout = contract.claim_winnings(bob(), market_id).unwrap();
        assert_eq!(bob_payout, 300); // Gets entire pool
        
        // Others have no winning shares
        assert!(contract.claim_winnings(alice(), market_id).is_err());
        assert!(contract.claim_winnings(charlie(), market_id).is_err());
    }

    #[test]
    fn test_invalid_option_index() {
        let mut contract = PredictionMarket::new(alice());
        contract.set_market_creator(alice(), market_creator()).unwrap();
        
        contract.create_market(
            market_creator(),
            "Test?".into(),
            vec!["A".into(), "B".into()],
            "Criteria".into(),
            "Source".into(),
            100,
        ).unwrap();
        
        // Try to bet on non-existent option
        assert!(contract.place_bet(alice(), 0, 5, 100).is_err());
    }
}
