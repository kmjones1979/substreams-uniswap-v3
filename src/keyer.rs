use crate::pb::uniswap;
use crate::utils;
use substreams::scalar::BigInt;
use substreams::Hex;

pub const UNISWAP_DAY_DATA: &str = "uniswap_day_data";
pub const POOL_DAY_DATA: &str = "pool_day_data";
pub const POOL_HOUR_DATA: &str = "pool_hour_data";

// ------------------------------------------------
//      store_pools_count
// ------------------------------------------------
pub fn factory_pool_count_key() -> String {
    format!("factory:poolCount")
}

// ------------------------------------------------
//      store_pools
// ------------------------------------------------
pub fn pool_key(pool_address: &String) -> String {
    format!("pool:{}", pool_address)
}

pub fn pool_token_index_key<T>(token0_address: T, token1_address: T, fee: u32) -> String
where
    T: AsRef<str>,
{
    format!(
        "index:{}:{}",
        generate_tokens_key(token0_address.as_ref(), token1_address.as_ref()),
        fee
    )
}

pub fn token_key(token_address: &String) -> String {
    format!("token:{}", token_address)
}

pub fn generate_tokens_key(token0: &str, token1: &str) -> String {
    if token0 > token1 {
        return format!("{}:{}", token1, token0);
    }
    return format!("{}:{}", token0, token1);
}

// ------------------------------------------------
//      store_tokens_whitelist_pools
// ------------------------------------------------
pub fn token_pool_whitelist(token_address: &String) -> String {
    format!("token:{}", token_address)
}

// ------------------------------------------------
//      store_pool_sqrt_price
// ------------------------------------------------
pub fn pool_sqrt_price_key(pool_address: &String) -> String {
    format!("sqrt_price:{}", pool_address)
}

pub fn pool_day_data_sqrt_price(pool_address: &String, day_id: String) -> String {
    format!("{}:{}:{}", POOL_DAY_DATA, pool_address, day_id)
}

// ------------------------------------------------
//      store_prices
// ------------------------------------------------
pub fn prices_pool_token_key(
    pool_address: &String,
    token_address: &String,
    token: String,
) -> String {
    format!("pool:{}:{}:{}", pool_address, token_address, token)
}

// TODO: is the naming here correct?
pub fn prices_token_pair(
    token_numerator_address: &String,
    token_denominator_address: &String,
) -> String {
    format!(
        "pair:{}:{}",
        token_numerator_address, token_denominator_address
    )
}

pub fn pool_day_data_token_price(pool_address: &String, token: String, day_id: String) -> String {
    format!("{}:{}:{}:{}", POOL_DAY_DATA, pool_address, token, day_id)
}

// ------------------------------------------------
//      store_totals
// ------------------------------------------------
pub fn factory_total_value_locked_eth() -> String {
    format!("factory:totalValueLockedETH")
}

pub fn factory_total_value_locked_usd() -> String {
    format!("factory:totalValueLockedUSD")
}

pub fn uniswap_total_value_locked_usd(day_id: String) -> String {
    format!("{}:{}", UNISWAP_DAY_DATA, day_id)
}

pub fn pool_day_data_total_value_locked_usd(pool_address: &String, day_id: String) -> String {
    format!("{}:{}:{}", POOL_DAY_DATA, pool_address, day_id)
}

// ------------------------------------------------
//      store_pool_fee_growth_global_x128
// ------------------------------------------------
pub fn pool_fee_growth_global_x128(pool_address: &String, token: String) -> String {
    format!("fee:{}:{}", pool_address, token)
}

pub fn pool_day_data_fee_growth_global_x128(
    pool_address: &String,
    token: String,
    day_id: String,
) -> String {
    format!("{}:{}:{}:{}", POOL_DAY_DATA, pool_address, token, day_id)
}

// ------------------------------------------------
//      store_total_value_locked
// ------------------------------------------------
pub fn token_usd_total_value_locked(token_address: &String) -> String {
    format!("token:{}:usd", token_address)
}

pub fn pool_eth_total_value_locked(pool_address: &String) -> String {
    format!("pool:{}:eth", pool_address)
}

pub fn pool_usd_total_value_locked(pool_address: &String) -> String {
    format!("pool:{}:usd", pool_address)
}

pub fn native_token_from_key(key: &String) -> Option<String> {
    let chunks: Vec<&str> = key.split(":").collect();
    if chunks.len() != 3 {
        return None;
    }
    if chunks[0] != "token" {
        return None;
    }
    return Some(chunks[1].to_string());
}

pub fn native_pool_from_key(key: &String) -> Option<(String, String)> {
    let chunks: Vec<&str> = key.split(":").collect();
    if chunks.len() != 4 {
        return None;
    }
    if chunks[0] != "pool" {
        return None;
    }
    return Some((chunks[1].to_string(), chunks[2].to_string()));
}

// ------------------------------------------------
//      store_native_total_value_locked
// ------------------------------------------------
pub fn token_native_total_value_locked(token_address: &String) -> String {
    format!("token:{}:native", token_address)
}

pub fn pool_native_total_value_locked_token(
    pool_address: &String,
    token_address: &String,
) -> String {
    format!("pool:{}:{}:native", pool_address, token_address)
}

pub fn factory_native_total_value_locked_eth() -> String {
    format!("factory:eth")
}

// ------------------------------------------------
//      store_pool_liquidities
// ------------------------------------------------
pub fn pool_liquidity(pool_address: &String) -> String {
    format!("liquidity:{}", pool_address)
}

pub fn pool_day_data_liquidity(pool_address: &String, day_id: String) -> String {
    format!("{}:{}:{}", POOL_DAY_DATA, pool_address, day_id)
}

// ------------------------------------------------
//      store_total_value_locked_by_tokens
// ------------------------------------------------
pub fn total_value_locked_by_pool(
    pool_address: &String,
    token_address: &String,
    token: String,
) -> String {
    format!("pool:{}:{}:{}", pool_address, token_address, token)
}

pub fn total_value_locked_by_token(token_address: &String) -> String {
    format!("token:{}", token_address)
}

// ------------------------------------------------
//      store_derived_eth_prices
// ------------------------------------------------
pub fn token_eth_price(token_address: &String) -> String {
    format!("token:{}:dprice:eth", token_address)
}

pub fn bundle_eth_price() -> String {
    format!("bundle")
}

// ------------------------------------------------
//      store_total_tx_counts
// ------------------------------------------------
pub fn pool_total_tx_count(pool_address: &String) -> String {
    format!("pool:{}", pool_address)
}

pub fn token_total_tx_count(token_address: &String) -> String {
    format!("token:{}", token_address)
}

pub fn factory_total_tx_count() -> String {
    format!("factory:{}", Hex(utils::UNISWAP_V3_FACTORY))
}

pub fn uniswap_day_data_tx_count(day_id: String) -> String {
    format!("{}:{}", UNISWAP_DAY_DATA, day_id)
}

pub fn pool_day_data_tx_count(pool_addr: &String, day_id: String) -> String {
    format!("{}:{}:{}", POOL_DAY_DATA, pool_addr, day_id)
}
pub fn pool_hour_data_tx_count(pool_addr: &String, hour_id: String) -> String {
    format!("{}:{}:{}", POOL_HOUR_DATA, pool_addr, hour_id)
}

// ------------------------------------------------
//      store_swaps_volume
// ------------------------------------------------
pub fn swap_volume_token_0(pool_address: &String) -> String {
    format!("swap:{}:volume:token0", pool_address)
}

pub fn swap_volume_token_1(pool_address: &String) -> String {
    format!("swap:{}:volume:token1", pool_address)
}

pub fn swap_volume_usd(pool_address: &String) -> String {
    format!("swap:{}:volume:usd", pool_address)
}

pub fn swap_untracked_volume_usd(pool_address: &String) -> String {
    format!("swap:{}:volume:untrackedUSD", pool_address)
}

pub fn swap_fee_usd(pool_address: &String) -> String {
    format!("swap:{}:feesUSD", pool_address)
}

pub fn swap_token_volume(token_address: &String, token: String) -> String {
    format!("token:{}:{}", token_address, token)
}

pub fn swap_token_volume_usd(token_address: &String) -> String {
    format!("token:{}:volume:usd", token_address)
}

pub fn swap_token_volume_untracked_volume_usd(token_address: &String) -> String {
    format!("token:{}:volume:untrackedUSD", token_address)
}

pub fn swap_token_fee_usd(token_address: &String) -> String {
    format!("token:{}:feesUSD", token_address)
}

pub fn swap_factory_total_fees_usd() -> String {
    format!("factory:totalFeesUSD")
}

pub fn swap_factory_total_volume_usd() -> String {
    format!("factory:totalVolumeUSD")
}

pub fn swap_factory_untracked_volume_usd() -> String {
    format!("factory:untrackedVolumeUSD")
}

pub fn swap_factory_total_volume_eth() -> String {
    format!("factory:totalVolumeETH")
}

pub fn swap_factory_total_fees_eth() -> String {
    format!("factory:totalFeesETH")
}

pub fn swap_uniswap_day_data_volume_eth(day_id: String) -> String {
    format!("{}:{}:volumeETH", UNISWAP_DAY_DATA, day_id)
}

pub fn swap_uniswap_day_data_volume_usd(day_id: String) -> String {
    format!("{}:{}:volumeUSD", UNISWAP_DAY_DATA, day_id)
}

pub fn swap_uniswap_day_data_fees_usd(day_id: String) -> String {
    format!("{}:{}:feesUSD", UNISWAP_DAY_DATA, day_id)
}

pub fn swap_pool_day_data_volume_usd(pool_addr: &String, day_id: String) -> String {
    format!("{}:{}:{}:volumeUSD", POOL_DAY_DATA, pool_addr, day_id)
}

// volumeToken0 and volumeToken1
pub fn swap_pool_day_data_volume_token(
    pool_addr: &String,
    day_id: String,
    token_addr: &String,
    volume_token_index: String,
) -> String {
    format!(
        "{}:{}:{}:{}:{}:",
        POOL_DAY_DATA, pool_addr, day_id, token_addr, volume_token_index
    )
}

pub fn swap_pool_day_data_fees_usd(pool_addr: &String, day_id: String) -> String {
    format!("{}:{}:{}:feesUSD", POOL_DAY_DATA, pool_addr, day_id)
}

// ------------------------------------------------
//      store_ticks
// ------------------------------------------------
pub fn ticks(key: &String) -> String {
    format!("tick:{}", key) // tick:{pool_addr}#tick_lower/tick_upper
}

// ------------------------------------------------
//      store_ticks_liquidities
// ------------------------------------------------
pub fn tick_liquidities_net(pool: &String, tick_idx: &uniswap::BigInt) -> String {
    let value = BigInt::from(tick_idx);
    format!("tick:{}:{}:liquidityNet", pool, value)
}

pub fn tick_liquidities_gross(pool: &String, tick_idx: &uniswap::BigInt) -> String {
    let value = BigInt::from(tick_idx);
    format!("tick:{}:{}:liquidityGross", pool, value)
}

// ------------------------------------------------
//      store_all_positions and store_positions
// ------------------------------------------------
pub fn all_position(id: &String, event_type: &String) -> String {
    format!("position:{}:{}", id, event_type)
}

// ------------------------------------------------
//      store_positions_misc
// ------------------------------------------------
pub fn position(id: &String, position_type: &String) -> String {
    format!("position:{}:{}", id, position_type)
}
pub fn position_liquidity(id: &String) -> String {
    format!("position:{}:liquidity", id)
}

pub fn position_deposited_token(id: &String, token: &str) -> String {
    format!("position:{}:deposited{}", id, token)
}

pub fn position_withdrawn_token(id: &String, token: &str) -> String {
    format!("position:{}:withdrawn{}", id, token)
}

pub fn position_collected_fees_token(id: &String, token: &str) -> String {
    format!("position:{}:collectedFees{}", id, token)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bigdecimal::BigDecimal;
    use prost::bytes::Buf;
    use std::str::FromStr;

    #[test]
    fn test_bigdecimal_from_string() {
        let bytes: [u8; 32] = [
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 4,
        ];
        let bytes_str = Hex(&bytes).to_string();
        println!("{}", bytes_str);
        let eql = bytes_str == "0000000000000000000000000000000000000000000000000000000000000004";
        assert_eq!(true, eql)
    }

    #[test]
    fn test_invalid_token_key() {
        let input = "pool:bb:aa".to_string();
        assert_eq!(None, native_token_from_key(&input));
    }

    #[test]
    fn test_invalid_pool_key() {
        let input = "token:bb".to_string();
        assert_eq!(None, native_pool_from_key(&input));
    }
}
