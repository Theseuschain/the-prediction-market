//! Price Tool Reference
//!
//! This file documents the GetPriceTool implementation that was added to the
//! tool-executor. The actual implementation lives in:
//!   - tool-executor/src/tools.rs (GetPriceTool struct and impl)
//!   - tool-executor/src/config.rs (PriceConfig struct)
//!   - tool-executor/config.yaml (price configuration)
//!
//! ## Tool Signature
//!
//! ```ship
//! tool get_price(asset: string) -> PriceData;
//!
//! struct PriceData {
//!     asset: string,
//!     coingecko_id: string,
//!     price_usd: number,
//!     timestamp: number,
//!     retrieved_at_iso: string
//! }
//! ```
//!
//! ## Usage in SHIP Agents
//!
//! ```ship
//! // Declare the tool
//! tool get_price(asset: string) -> PriceData;
//!
//! // Use in a node
//! node check_price() {
//!     let btc = get_price("bitcoin");
//!     if (btc.price_usd > 100000) {
//!         // ...
//!     }
//! }
//! ```
//!
//! ## Supported Assets
//!
//! Common symbols are automatically normalized:
//! - btc, bitcoin → bitcoin
//! - eth, ethereum → ethereum
//! - sol, solana → solana
//! - doge, dogecoin → dogecoin
//! - etc.
//!
//! You can also use CoinGecko IDs directly (e.g., "matic-network", "avalanche-2").
//!
//! ## Rate Limiting
//!
//! CoinGecko free tier allows ~30 requests/minute.
//! The tool-executor respects the configured rate_limit_ms (default: 1000ms).
//!
//! ## Configuration
//!
//! In tool-executor/config.yaml:
//!
//! ```yaml
//! tools:
//!   price:
//!     base_url: "https://api.coingecko.com/api/v3"
//!     rate_limit_ms: 1000
//! ```
