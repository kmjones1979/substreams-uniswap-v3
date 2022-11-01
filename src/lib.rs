pub mod abi;
mod db;
mod eth;
mod keyer;
mod math;
mod pb;
mod price;
mod rpc;
mod utils;
mod generated;
use substreams::prelude::*;

use crate::abi::pool::events::Swap;
use crate::ethpb::v2::{Block, StorageChange};
use crate::pb::position_event::PositionEventType;
use crate::pb::uniswap_types_v1::event::Type::{Burn as BurnEvent, Mint as MintEvent, Swap as SwapEvent};
use crate::pb::uniswap_types_v1::tick::Origin::{Burn, Mint};
use crate::pb::uniswap_types_v1::tick::Type::{Lower, Upper};
use crate::pb::uniswap_types_v1::{
    Erc20Token, Erc20Tokens, Event, EventAmount, Events, Pool, PoolLiquidities, PoolLiquidity,
    PoolSqrtPrice, PoolSqrtPrices, Pools, Tick, Ticks,
};
use crate::pb::{uniswap_types_v1, PositionEvent};
use crate::price::WHITELIST_TOKENS;
use crate::uniswap_types_v1::position::PositionType;
use crate::uniswap_types_v1::position::PositionType::{
    Collect, DecreaseLiquidity, IncreaseLiquidity, Transfer,
};
use crate::uniswap_types_v1::{
    Flash, Flashes, Position, Positions, SnapshotPosition, SnapshotPositions, Transactions,
};
use crate::utils::{NON_FUNGIBLE_POSITION_MANAGER, UNISWAP_V3_FACTORY};
use std::collections::HashMap;
use std::ops::{Add, Div, Mul, Sub};
use substreams::errors::Error;
use substreams::hex;
use substreams::pb::substreams::Clock;
use substreams::prelude::*;
use substreams::scalar::{BigDecimal, BigInt};
use substreams::store;
use substreams::store::{DeltaArray, DeltaBigDecimal, DeltaBigInt, DeltaProto, StoreAddBigDecimal, StoreAddBigInt, StoreAppend, StoreGetBigDecimal, StoreGetBigInt, StoreGetProto, StoreGetRaw, StoreSetBigDecimal, StoreSetBigInt, StoreSetProto, StoreSetString};
use substreams::{log, Hex};
use substreams_entity_change::pb::entity::EntityChanges;
use substreams_ethereum::{pb::eth as ethpb, Event as EventTrait};

impl generated::substreams::SubstreamsTrait for generated::substreams::Substreams {

   fn map_pools_created(block: Block) -> Result<Pools, Error> {
        use abi::factory::events::PoolCreated;

        Ok(Pools {
            pools: block
                .events::<PoolCreated>(&[&UNISWAP_V3_FACTORY])
                .filter_map(|(event, log)| {
                    log::info!("asd");
                    log::info!("pool addr: {}", Hex(&event.pool));

                    let token0_address: String = Hex(&event.token0).to_string();
                    let token1_address: String = Hex(&event.token1).to_string();

                    //todo: question regarding the ignore_pool line. In the
                    // uniswap-v3 subgraph, they seem to bail out when they
                    // match the addr, should we do the same ?
                    Some(Pool {
                        address: Hex(&log.data()[44..64]).to_string(),
                        transaction_id: Hex(&log.receipt.transaction.hash).to_string(),
                        created_at_block_number: block.number,
                        created_at_timestamp: block.timestamp_seconds(),
                        fee_tier: Some(event.fee.into()),
                        tick_spacing: event.tick_spacing.into(),
                        log_ordinal: log.ordinal(),
                        ignore_pool: event.pool == hex!("8fe8d9bb8eeba3ed688069c3d6b556c9ca258248"),
                        token0: Some(match rpc::create_uniswap_token(&token0_address) {
                            Some(mut token) => {
                                token.total_supply = rpc::token_total_supply_call(&token0_address)
                                    .expect("failed to get token0 total supply")
                                    .to_string();
                                token
                            }
                            None => {
                                // We were unable to create the uniswap token, so we discard this event entirely
                                return None;
                            }
                        }),
                        token1: Some(match rpc::create_uniswap_token(&token1_address) {
                            Some(mut token) => {
                                token.total_supply = rpc::token_total_supply_call(&token1_address)
                                    .expect("failed to get token1 total supply")
                                    .to_string();
                                token
                            }
                            None => {
                                // We were unable to create the uniswap token, so we discard this event entirely
                                return None;
                            }
                        }),
                        ..Default::default()
                    })
                })
                .collect(),
        })
    }


   fn store_pools(pools: Pools, store: StoreSetProto<Pool>) {
        for pool in pools.pools {
            store.set(pool.log_ordinal, keyer::pool_key(&pool.address), &pool);

            store.set(
                pool.log_ordinal,
                keyer::pool_token_index_key(
                    &pool.token0_ref().address(),
                    &pool.token1_ref().address(),
                    pool.fee_tier.as_ref().unwrap().into(),
                ),
                &pool,
            );
        }
    }


   fn store_pool_count(pools: Pools, store: StoreAddBigInt) {
        for pool in pools.pools {
            store.add(
                pool.log_ordinal,
                keyer::factory_pool_count_key(),
                &BigInt::one(),
            )
        }
    }


   fn map_tokens_whitelist_pools(pools: Pools) -> Result<Erc20Tokens, Error> {
        let mut tokens = vec![];

        for pool in pools.pools {
            let mut token0 = pool.token0();
            let mut token1 = pool.token1();

            let token0_whitelisted = WHITELIST_TOKENS.contains(&token0.address.as_str());
            let token1_whitelisted = WHITELIST_TOKENS.contains(&token1.address.as_str());

            if token0_whitelisted {
                log::info!("adding pool: {} to token: {}", pool.address, token1.address);
                token1.whitelist_pools.push(pool.address.to_string());
                tokens.push(token1);
            }

            if token1_whitelisted {
                log::info!("adding pool: {} to token: {}", pool.address, token0.address);
                token0.whitelist_pools.push(pool.address.to_string());
                tokens.push(token0);
            }
        }

        Ok(Erc20Tokens { tokens })
    }


   fn store_tokens_whitelist_pools(tokens: Erc20Tokens, output_append: StoreAppend<String>) {
        for token in tokens.tokens {
            output_append.append_all(
                1,
                keyer::token_pool_whitelist(&token.address),
                token.whitelist_pools,
            );
        }
    }


   fn map_pool_sqrt_price(
        block: Block,
        pools_store: StoreGetProto<Pool>,
    ) -> Result<PoolSqrtPrices, Error> {
        let mut pool_sqrt_prices = vec![];
        for log in block.logs() {
            let pool_address = &Hex(log.address()).to_string();
            if let Some(event) = abi::pool::events::Initialize::match_and_decode(log) {
                log::info!(
                "log addr: {}",
                Hex(&log.receipt.transaction.hash.as_slice()).to_string()
            );
                match pools_store.get_last(&keyer::pool_key(&pool_address)) {
                    None => {
                        log::debug!("pool {} does not exist", pool_address);
                        continue;
                    }
                    Some(pool) => {
                        pool_sqrt_prices.push(PoolSqrtPrice {
                            pool_address: pool.address,
                            ordinal: log.ordinal(),
                            sqrt_price: Some(event.sqrt_price_x96.into()),
                            tick: Some(event.tick.into()),
                        });
                    }
                }
            } else if let Some(event) = Swap::match_and_decode(log) {
                log::info!(
                "log addr: {}",
                Hex(&log.receipt.transaction.hash.as_slice()).to_string()
            );
                match pools_store.get_last(&keyer::pool_key(&pool_address)) {
                    None => {
                        log::debug!("pool {} does not exist", pool_address);
                        continue;
                    }
                    Some(pool) => {
                        pool_sqrt_prices.push(PoolSqrtPrice {
                            pool_address: pool.address,
                            ordinal: log.ordinal(),
                            sqrt_price: Some(event.sqrt_price_x96.into()),
                            tick: Some(event.tick.into()),
                        });
                    }
                }
            }
        }
        Ok(PoolSqrtPrices { pool_sqrt_prices })
    }

   fn store_pool_sqrt_price(sqrt_prices: PoolSqrtPrices, store: StoreSetProto<PoolSqrtPrice>) {
        for sqrt_price in sqrt_prices.pool_sqrt_prices {
            store.set(
                sqrt_price.ordinal,
                keyer::pool_sqrt_price_key(&sqrt_price.pool_address),
                &sqrt_price,
            )
        }
    }


    fn store_prices(
         pool_sqrt_prices: PoolSqrtPrices,
         pools_store: StoreGetProto<Pool>,
         store: StoreSetBigDecimal,
     ) {
         for sqrt_price_update in pool_sqrt_prices.pool_sqrt_prices {
             match pools_store.get_last(keyer::pool_key(&sqrt_price_update.pool_address)) {
                 None => {
                     log::info!("skipping pool {}", &sqrt_price_update.pool_address,);
                     continue;
                 }
                 Some(pool) => {
                     let token0 = pool.token0.as_ref().unwrap();
                     let token1 = pool.token1.as_ref().unwrap();
                     log::debug!(
                     "pool addr: {}, pool trx_id: {}, token 0 addr: {}, token 1 addr: {}",
                     pool.address,
                     pool.transaction_id,
                     token0.address,
                     token1.address
                 );

                     let sqrt_price = BigDecimal::from(sqrt_price_update.sqrt_price.unwrap());
                     log::debug!("sqrtPrice: {}", sqrt_price.to_string());

                     let tokens_price: (BigDecimal, BigDecimal) =
                         price::sqrt_price_x96_to_token_prices(sqrt_price, &token0, &token1);
                     log::debug!("token prices: {} {}", tokens_price.0, tokens_price.1);

                     store.set_many(
                         sqrt_price_update.ordinal,
                         &vec![
                             keyer::prices_pool_token_key(
                                 &pool.address,
                                 &token0.address,
                                 "token0".to_string(),
                             ),
                             keyer::prices_token_pair(
                                 &pool.token0.as_ref().unwrap().address,
                                 &pool.token1.as_ref().unwrap().address,
                             ),
                         ],
                         &tokens_price.0,
                     );

                     store.set_many(
                         sqrt_price_update.ordinal,
                         &vec![
                             keyer::prices_pool_token_key(
                                 &pool.address,
                                 &token1.address,
                                 "token1".to_string(),
                             ),
                             keyer::prices_token_pair(
                                 &pool.token1.as_ref().unwrap().address,
                                 &pool.token0.as_ref().unwrap().address,
                             ),
                         ],
                         &tokens_price.1,
                     );
                 }
             }
         }
     }


    fn map_pool_liquidities(
         block: Block,
         pools_store: StoreGetProto<Pool>,
     ) -> Result<PoolLiquidities, Error> {
         let mut pool_liquidities = vec![];
         for trx in block.transaction_traces {
             if trx.status != 1 {
                 continue;
             }
             for call in trx.calls {
                 let _call_index = call.index;
                 if call.state_reverted {
                     continue;
                 }
                 for log in call.logs {
                     let pool_key = keyer::pool_key(&Hex(&log.address).to_string());
                     if let Some(_) = Swap::match_and_decode(&log) {
                         log::debug!("swagp - trx_id: {}", Hex(&trx.hash).to_string());
                         match pools_store.get_last(&pool_key) {
                             None => continue,
                             Some(pool) => {
                                 if !pool.should_handle_swap() {
                                     continue;
                                 }
                                 if let Some(pl) = utils::extract_pool_liquidity(
                                     log.ordinal,
                                     &log.address,
                                     &call.storage_changes,
                                 ) {
                                     pool_liquidities.push(pl)
                                 }
                             }
                         }
                     } else if let Some(_) = abi::pool::events::Mint::match_and_decode(&log) {
                         log::debug!("mint - trx_id: {}", Hex(&trx.hash).to_string());
                         match pools_store.get_last(&pool_key) {
                             None => {
                                 log::info!("unknown pool");
                                 continue;
                             }
                             Some(pool) => {
                                 if !pool.should_handle_mint_and_burn() {
                                     continue;
                                 }
                                 if let Some(pl) = utils::extract_pool_liquidity(
                                     log.ordinal,
                                     &log.address,
                                     &call.storage_changes,
                                 ) {
                                     pool_liquidities.push(pl)
                                 }
                             }
                         }
                     } else if let Some(_) = abi::pool::events::Burn::match_and_decode(&log) {
                         log::debug!("burn - trx_id: {}", Hex(&trx.hash).to_string());
                         match pools_store.get_last(&pool_key) {
                             None => continue,
                             Some(pool) => {
                                 if !pool.should_handle_mint_and_burn() {
                                     continue;
                                 }
                                 if let Some(pl) = utils::extract_pool_liquidity(
                                     log.ordinal,
                                     &log.address,
                                     &call.storage_changes,
                                 ) {
                                     pool_liquidities.push(pl)
                                 }
                             }
                         }
                     }
                 }
             }
         }
         Ok(PoolLiquidities { pool_liquidities })
     }


    fn store_pool_liquidities(pool_liquidities: PoolLiquidities, store: StoreSetBigInt) {
         for pool_liquidity in pool_liquidities.pool_liquidities {
             let big_int: BigInt = pool_liquidity.liquidity.unwrap().into();
             store.set(
                 0,
                 keyer::pool_liquidity(&pool_liquidity.pool_address),
                 &big_int,
             )
         }
     }


   fn map_swaps_mints_burns(
        block: Block,
        pools_store: StoreGetProto<Pool>,
    ) -> Result<Events, Error> {
        let mut events = vec![];
        for log in block.logs() {
            let pool_key = &format!("pool:{}", Hex(&log.address()).to_string());

            if let Some(swap) = Swap::match_and_decode(log) {
                match pools_store.get_last(pool_key) {
                    None => {
                        log::info!(
                        "invalid swap. pool does not exist. pool address {} transaction {}",
                        Hex(&log.address()).to_string(),
                        Hex(&log.receipt.transaction.hash).to_string()
                    );
                        continue;
                    }
                    Some(pool) => {
                        if !pool.should_handle_swap() {
                            continue;
                        }

                        let token0 = pool.token0.as_ref().unwrap();
                        let token1 = pool.token1.as_ref().unwrap();

                        let amount0 = swap.amount0.to_decimal(token0.decimals);
                        let amount1 = swap.amount1.to_decimal(token1.decimals);
                        log::debug!("amount0: {}, amount1:{}", amount0, amount1);

                        events.push(Event {
                            log_ordinal: log.ordinal(),
                            log_index: log.block_index() as u64,
                            pool_address: pool.address.to_string(),
                            token0: token0.address.clone(),
                            token1: token1.address.clone(),
                            fee: pool.fee_tier.unwrap().value,
                            transaction_id: Hex(&log.receipt.transaction.hash).to_string(),
                            timestamp: block.timestamp_seconds(),
                            created_at_block_number: block.number,
                            r#type: Some(SwapEvent(uniswap_types_v1::Swap {
                                sender: Hex(&swap.sender).to_string(),
                                recipient: Hex(&swap.recipient).to_string(),
                                origin: Hex(&log.receipt.transaction.from).to_string(),
                                amount_0: Some(uniswap_types_v1::BigDecimal {
                                    value: amount0.to_string(),
                                }),
                                amount_1: Some(uniswap_types_v1::BigDecimal {
                                    value: amount1.to_string(),
                                }),
                                sqrt_price: Some(swap.sqrt_price_x96.into()),
                                liquidity: Some(swap.liquidity.into()),
                                tick: Some(swap.tick.into()),
                            })),
                        });
                    }
                }
            } else if let Some(mint) = abi::pool::events::Mint::match_and_decode(log) {
                match pools_store.get_last(pool_key) {
                    None => {
                        log::info!(
                        "invalid mint. pool does not exist. pool address {} transaction {}",
                        Hex(&log.address()).to_string(),
                        Hex(&log.receipt.transaction.hash).to_string()
                    );
                        continue;
                    }
                    Some(pool) => {
                        if !pool.should_handle_mint_and_burn() {
                            continue;
                        }

                        let token0 = pool.token0.as_ref().unwrap();
                        let token1 = pool.token1.as_ref().unwrap();

                        let amount0 = mint.amount0.to_decimal(token0.decimals);
                        let amount1 = mint.amount1.to_decimal(token1.decimals);
                        log::debug!(
                        "logOrdinal: {}, amount0: {}, amount1:{}",
                        log.ordinal(),
                        amount0,
                        amount1
                    );

                        events.push(Event {
                            log_ordinal: log.ordinal(),
                            log_index: log.block_index() as u64,
                            pool_address: pool.address.to_string(),
                            token0: token0.address.clone(),
                            token1: token1.address.clone(),
                            fee: pool.fee_tier.unwrap().value,
                            transaction_id: Hex(&log.receipt.transaction.hash).to_string(),
                            timestamp: block.timestamp_seconds(),
                            created_at_block_number: block.number,
                            r#type: Some(MintEvent(uniswap_types_v1::Mint {
                                owner: Hex(&mint.owner).to_string(),
                                sender: Hex(&mint.sender).to_string(),
                                origin: Hex(&log.receipt.transaction.from).to_string(),
                                amount: Some(mint.amount.into()),
                                amount_0: Some(amount0.into()),
                                amount_1: Some(amount1.into()),
                                tick_lower: Some(mint.tick_lower.into()),
                                tick_upper: Some(mint.tick_upper.into()),
                            })),
                        });
                    }
                }
            } else if let Some(burn) = abi::pool::events::Burn::match_and_decode(log) {
                match pools_store.get_last(pool_key) {
                    None => {
                        log::info!(
                        "invalid burn. pool does not exist. pool address {} transaction {}",
                        Hex(&log.address()).to_string(),
                        Hex(&log.receipt.transaction.hash).to_string()
                    );
                        continue;
                    }
                    Some(pool) => {
                        if !pool.should_handle_mint_and_burn() {
                            continue;
                        }

                        let token0 = pool.token0.as_ref().unwrap();
                        let token1 = pool.token1.as_ref().unwrap();

                        let amount0_bi: BigInt = burn.amount0;
                        let amount1_bi: BigInt = burn.amount1;
                        let amount0 = amount0_bi.to_decimal(token0.decimals);
                        let amount1 = amount1_bi.to_decimal(token1.decimals);
                        log::debug!("amount0: {}, amount1:{}", amount0, amount1);

                        events.push(Event {
                            log_ordinal: log.ordinal(),
                            log_index: log.block_index() as u64,
                            pool_address: pool.address.to_string(),
                            token0: token1.address.clone(),
                            token1: token1.address.clone(),
                            fee: pool.fee_tier.unwrap().value,
                            transaction_id: Hex(&log.receipt.transaction.hash).to_string(),
                            timestamp: block.timestamp_seconds(),
                            created_at_block_number: block.number,
                            r#type: Some(BurnEvent(uniswap_types_v1::Burn {
                                owner: Hex(&burn.owner).to_string(),
                                origin: Hex(&log.receipt.transaction.from).to_string(),
                                amount: Some(burn.amount.into()),
                                amount_0: Some(amount0.into()),
                                amount_1: Some(amount1.into()),
                                tick_lower: Some(burn.tick_lower.into()),
                                tick_upper: Some(burn.tick_upper.into()),
                            })),
                        });
                    }
                }
            }
        }
        Ok(Events { events })
    }


   fn map_event_amounts(events: Events) -> Result<uniswap_types_v1::EventAmounts, Error> {
        let mut event_amounts = vec![];
        for event in events.events {
            log::debug!("transaction id: {}", event.transaction_id);
            if event.r#type.is_none() {
                continue;
            }

            if event.r#type.is_some() {
                match event.r#type.unwrap() {
                    BurnEvent(burn) => {
                        log::debug!("handling burn for pool {}", event.pool_address);
                        let amount0: BigDecimal = burn.amount_0.unwrap().into();
                        let amount1: BigDecimal = burn.amount_1.unwrap().into();
                        event_amounts.push(EventAmount {
                            pool_address: event.pool_address,
                            log_ordinal: event.log_ordinal,
                            token0_addr: event.token0,
                            amount0_value: Some(amount0.neg().into()),
                            token1_addr: event.token1,
                            amount1_value: Some(amount1.neg().into()),
                            ..Default::default()
                        });
                    }
                    MintEvent(mint) => {
                        log::debug!("handling mint for pool {}", event.pool_address);
                        let amount0: BigDecimal = mint.amount_0.unwrap().into();
                        let amount1: BigDecimal = mint.amount_1.unwrap().into();
                        event_amounts.push(EventAmount {
                            pool_address: event.pool_address,
                            log_ordinal: event.log_ordinal,
                            token0_addr: event.token0,
                            amount0_value: Some(amount0.into()),
                            token1_addr: event.token1,
                            amount1_value: Some(amount1.into()),
                            ..Default::default()
                        });
                    }
                    SwapEvent(swap) => {
                        log::debug!("handling swap for pool {}", event.pool_address);
                        let amount0: BigDecimal = swap.amount_0.unwrap().into();
                        let amount1: BigDecimal = swap.amount_1.unwrap().into();
                        event_amounts.push(EventAmount {
                            pool_address: event.pool_address,
                            log_ordinal: event.log_ordinal,
                            token0_addr: event.token0,
                            amount0_value: Some(amount0.into()),
                            token1_addr: event.token1,
                            amount1_value: Some(amount1.into()),
                            ..Default::default()
                        });
                    }
                }
            }
        }
        Ok(uniswap_types_v1::EventAmounts { event_amounts })
    }


   fn map_transactions(
        block: Block,
        pools_store: StoreGetProto<Pool>,
    ) -> Result<Transactions, Error> {
        let mut transactions: Transactions = Transactions {
            transactions: vec![],
        };

        for log in block.logs() {
            let mut add_transaction = false;
            let pool_key = &format!("pool:{}", Hex(&log.address()).to_string());

            if let Some(_) = abi::pool::events::Burn::match_and_decode(log) {
                match pools_store.get_last(pool_key) {
                    None => continue,

                    Some(_) => add_transaction = true,
                }
            } else if let Some(_) = abi::pool::events::Mint::match_and_decode(log) {
                match pools_store.get_last(pool_key) {
                    None => continue,
                    Some(_) => add_transaction = true,
                }
            } else if let Some(_) = Swap::match_and_decode(log) {
                match pools_store.get_last(pool_key) {
                    None => continue,
                    Some(_) => add_transaction = true,
                }
            } else if let Some(_) =
            abi::positionmanager::events::IncreaseLiquidity::match_and_decode(log)
            {
                if log.address() == NON_FUNGIBLE_POSITION_MANAGER {
                    add_transaction = true;
                }
            } else if let Some(_) = abi::positionmanager::events::Collect::match_and_decode(log) {
                if log.address() == NON_FUNGIBLE_POSITION_MANAGER {
                    add_transaction = true;
                }
            } else if let Some(_) =
            abi::positionmanager::events::DecreaseLiquidity::match_and_decode(log)
            {
                if log.address() == NON_FUNGIBLE_POSITION_MANAGER {
                    add_transaction = true;
                }
            } else if let Some(_) = abi::positionmanager::events::Transfer::match_and_decode(log) {
                if log.address() == NON_FUNGIBLE_POSITION_MANAGER {
                    add_transaction = true;
                }
            }

            if add_transaction {
                transactions.transactions.push(utils::load_transaction(
                    block.number,
                    block.timestamp_seconds(),
                    log.ordinal(),
                    log.receipt.transaction,
                ));
            }
        }

        Ok(transactions)
    }


   fn store_totals(
        clock: Clock,
        store_eth_prices: StoreGetBigDecimal,
        total_value_locked_deltas: store::Deltas<DeltaBigDecimal>,
        store: StoreAddBigDecimal,
    ) {
        let timestamp_seconds = clock.timestamp.unwrap().seconds;
        let day_id: i64 = timestamp_seconds / 86400;
        store.delete_prefix(0, &format!("uniswap_day_data:{}:", day_id - 1));

        let mut pool_total_value_locked_eth_new_value: BigDecimal = BigDecimal::zero();
        for delta in total_value_locked_deltas.deltas {
            if !delta.key.starts_with("pool:") {
                continue;
            }
            match delta.key.as_str().split(":").last().unwrap() {
                "eth" => {
                    let pool_total_value_locked_eth_old_value = delta.old_value;
                    // let pool_total_value_locked_eth_old_value: BigDecimal = math::decimal_from_bytes(&delta.old_value);
                    pool_total_value_locked_eth_new_value = delta.new_value;
                    // pool_total_value_locked_eth_new_value = math::decimal_from_bytes(&delta.new_value);

                    let pool_total_value_locked_eth_diff: BigDecimal =
                        pool_total_value_locked_eth_old_value
                            .sub(pool_total_value_locked_eth_new_value.clone());

                    store.add(
                        delta.ordinal,
                        keyer::factory_total_value_locked_eth(),
                        &pool_total_value_locked_eth_diff,
                    )
                }
                "usd" => {
                    let bundle_eth_price: BigDecimal = match store_eth_prices.get_last("bundle") {
                        None => continue,
                        Some(price) => price,
                    };
                    log::debug!("eth_price_usd: {}", bundle_eth_price);

                    let total_value_locked_usd: BigDecimal = pool_total_value_locked_eth_new_value
                        .clone()
                        .mul(bundle_eth_price);

                    // here we have to do a hackish way to set the value, to not have to
                    // create a new store which would do the same but that would set the
                    // value instead of summing it, what we do is calculate the difference
                    // and simply add/sub the difference and that mimics the same as setting
                    // the value
                    let total_value_locked_usd_old_value: BigDecimal = delta.old_value;
                    // let total_value_locked_usd_old_value: BigDecimal = math::decimal_from_bytes(&delta.old_value);
                    let diff: BigDecimal = total_value_locked_usd
                        .clone()
                        .sub(total_value_locked_usd_old_value);

                    store.add(
                        delta.ordinal,
                        keyer::factory_total_value_locked_usd(),
                        &diff,
                    );
                    store.add(
                        delta.ordinal,
                        keyer::uniswap_total_value_locked_usd(day_id.to_string()),
                        &total_value_locked_usd,
                    )
                }
                _ => continue,
            }
        }
    }


   fn store_total_tx_counts(clock: Clock, events: Events, output: StoreAddBigInt) {
        let timestamp_seconds = clock.timestamp.unwrap().seconds;
        let day_id: i64 = timestamp_seconds / 86400;
        output.delete_prefix(0, &format!("uniswap_day_data:{}:", day_id - 1));

        for event in events.events {
            let keys: Vec<String> = vec![
                keyer::pool_total_tx_count(&event.pool_address),
                keyer::token_total_tx_count(&event.token0),
                keyer::token_total_tx_count(&event.token1),
                keyer::factory_total_tx_count(),
                keyer::uniswap_data_data_tx_count(day_id.to_string()),
            ];
            output.add_many(event.log_ordinal, &keys, &BigInt::from(1 as i32));
        }
    }


   fn store_swaps_volume(
        clock: Clock,
        events: Events,
        store_pool: StoreGetProto<Pool>,
        store_total_tx_counts: StoreGetBigInt,
        store_eth_prices: StoreGetBigDecimal,
        output: StoreAddBigDecimal,
    ) {
        let timestamp_seconds = clock.timestamp.unwrap().seconds;
        let day_id: i64 = timestamp_seconds / 86400;
        output.delete_prefix(0, &format!("uniswap_day_data:{}:", day_id - 1));

        for event in events.events {
            let pool: Pool = match store_pool.get_last(keyer::pool_key(&event.pool_address)) {
                None => continue,
                Some(pool) => pool,
            };
            match store_total_tx_counts.get_last(keyer::pool_total_tx_count(&event.pool_address)) {
                None => {}
                Some(_) => match event.r#type.unwrap() {
                    SwapEvent(swap) => {
                        let eth_price_in_usd: BigDecimal =
                            match store_eth_prices.get_last(&keyer::bundle_eth_price()) {
                                None => {
                                    panic!("bundle eth price not found")
                                }
                                Some(price) => price,
                            };

                        let token0_derived_eth_price: BigDecimal =
                            match store_eth_prices.get_last(keyer::token_eth_price(&event.token0)) {
                                None => continue,
                                Some(price) => price,
                            };

                        let token1_derived_eth_price: BigDecimal =
                            match store_eth_prices.get_last(keyer::token_eth_price(&event.token1)) {
                                None => continue,
                                Some(price) => price,
                            };

                        let mut amount0_abs: BigDecimal = BigDecimal::from(swap.amount_0.unwrap());
                        if amount0_abs.lt(&BigDecimal::from(0 as u64)) {
                            amount0_abs = amount0_abs.mul(BigDecimal::from(-1 as i64))
                        }

                        let mut amount1_abs: BigDecimal = BigDecimal::from(swap.amount_1.unwrap());
                        if amount1_abs.lt(&BigDecimal::from(0 as u64)) {
                            amount1_abs = amount1_abs.mul(BigDecimal::from(-1 as i64))
                        }

                        log::debug!("trx_id: {}", event.transaction_id);
                        log::debug!("bundle.ethPriceUSD: {}", eth_price_in_usd);
                        log::debug!("token0_derived_eth_price: {}", token0_derived_eth_price);
                        log::debug!("token1_derived_eth_price: {}", token1_derived_eth_price);
                        log::debug!("amount0_abs: {}", amount0_abs);
                        log::debug!("amount1_abs: {}", amount1_abs);

                        let amount_total_usd_tracked: BigDecimal = utils::get_tracked_amount_usd(
                            &event.token0,
                            &event.token1,
                            &token0_derived_eth_price,
                            &token1_derived_eth_price,
                            &amount0_abs,
                            &amount1_abs,
                            &eth_price_in_usd,
                        )
                            .div(BigDecimal::from(2 as i32));

                        let amount_total_eth_tracked =
                            math::safe_div(&amount_total_usd_tracked, &eth_price_in_usd);

                        let amount_total_usd_untracked: BigDecimal = amount0_abs
                            .clone()
                            .add(amount1_abs.clone())
                            .div(BigDecimal::from(2 as i32));

                        let fee_tier: BigDecimal = BigDecimal::from(pool.fee_tier.unwrap());
                        let fee_usd: BigDecimal = amount_total_usd_tracked
                            .clone()
                            .mul(fee_tier.clone())
                            .div(BigDecimal::from(1000000 as u64));
                        let fee_eth: BigDecimal = amount_total_eth_tracked
                            .clone()
                            .mul(fee_tier)
                            .div(BigDecimal::from(1000000 as u64));

                        output.add_many(
                            event.log_ordinal,
                            &vec![
                                keyer::swap_volume_token_0(&event.pool_address),
                                keyer::swap_token_volume(&event.token0, "token0".to_string()),
                            ],
                            &amount0_abs,
                        );
                        output.add_many(
                            event.log_ordinal,
                            &vec![
                                keyer::swap_volume_token_1(&event.pool_address),
                                keyer::swap_token_volume(&event.token1, "token1".to_string()),
                            ],
                            &amount1_abs,
                        );
                        output.add_many(
                            event.log_ordinal,
                            &vec![
                                keyer::swap_volume_usd(&event.pool_address),
                                keyer::swap_token_volume_usd(&event.token0),
                                keyer::swap_token_volume_usd(&event.token1),
                                keyer::swap_factory_total_volume_usd(),
                                keyer::swap_uniswap_day_data_volume_usd(day_id.to_string()),
                            ],
                            &amount_total_usd_tracked,
                        );
                        output.add_many(
                            event.log_ordinal,
                            &vec![
                                keyer::swap_untracked_volume_usd(&event.pool_address),
                                keyer::swap_token_volume_untracked_volume_usd(&event.token0),
                                keyer::swap_token_volume_untracked_volume_usd(&event.token1),
                                keyer::swap_factory_untracked_volume_usd(),
                            ],
                            &amount_total_usd_untracked,
                        );
                        output.add_many(
                            event.log_ordinal,
                            &vec![
                                keyer::swap_fee_usd(&event.pool_address),
                                keyer::swap_token_fee_usd(&event.token0),
                                keyer::swap_token_fee_usd(&event.token1),
                                keyer::swap_factory_total_fees_usd(),
                                keyer::swap_uniswap_day_data_fees_usd(day_id.to_string()),
                            ],
                            &fee_usd,
                        );
                        output.add(
                            event.log_ordinal,
                            keyer::swap_factory_total_volume_eth(),
                            &amount_total_eth_tracked.clone(),
                        );
                        output.add(
                            event.log_ordinal,
                            keyer::swap_factory_total_fees_eth(),
                            &fee_eth,
                        );
                        output.add(
                            event.log_ordinal,
                            keyer::swap_uniswap_day_data_volume_eth(day_id.to_string()),
                            &amount_total_eth_tracked,
                        );
                    }
                    _ => {}
                },
            }
        }
    }


   fn store_pool_fee_growth_global_x128(pools: Pools, store: StoreSetBigInt) {
        for pool in pools.pools {
            log::info!(
            "pool address: {} trx_id:{}",
            pool.address,
            pool.transaction_id
        );
            let (big_int_1, big_int_2) = rpc::fee_growth_global_x128_call(&pool.address);
            log::debug!("big decimal0: {}", big_int_1);
            log::debug!("big decimal1: {}", big_int_2);

            store.set(
                pool.log_ordinal,
                keyer::pool_fee_growth_global_x128(&pool.address, "token0".to_string()),
                &big_int_1,
            );
            store.set(
                pool.log_ordinal,
                keyer::pool_fee_growth_global_x128(&pool.address, "token1".to_string()),
                &big_int_2,
            );
        }
    }


   fn store_native_total_value_locked(
        event_amounts: uniswap_types_v1::EventAmounts,
        store: StoreAddBigDecimal,
    ) {
        for event_amount in event_amounts.event_amounts {
            let amount0: BigDecimal = event_amount.amount0_value.unwrap().into();
            let amount1: BigDecimal = event_amount.amount1_value.unwrap().into();
            store.add_many(
                event_amount.log_ordinal,
                &vec![
                    keyer::token_native_total_value_locked(&event_amount.token0_addr),
                    keyer::pool_native_total_value_locked_token(
                        &event_amount.pool_address,
                        &event_amount.token0_addr,
                    ),
                ],
                &amount0,
            );
            store.add_many(
                event_amount.log_ordinal,
                &vec![
                    keyer::token_native_total_value_locked(&event_amount.token1_addr),
                    keyer::pool_native_total_value_locked_token(
                        &event_amount.pool_address,
                        &event_amount.token1_addr,
                    ),
                ],
                &amount1,
            );
        }
    }


   fn store_eth_prices(
        pool_sqrt_prices: PoolSqrtPrices,
        pools_store: StoreGetProto<Pool>,
        prices_store: StoreGetBigDecimal,
        tokens_whitelist_pools_store: StoreGetRaw,
        total_native_value_locked_store: StoreGetBigDecimal,
        pool_liquidities_store: StoreGetBigInt,
        store: StoreSetBigDecimal,
    ) {
        for pool_sqrt_price in pool_sqrt_prices.pool_sqrt_prices {
            log::debug!(
            "handling pool price update - addr: {} price: {}",
            pool_sqrt_price.pool_address,
            pool_sqrt_price.sqrt_price.unwrap().value
        );
            let pool = pools_store.must_get_last(&keyer::pool_key(&pool_sqrt_price.pool_address));
            let token_0 = pool.token0.as_ref().unwrap();
            let token_1 = pool.token1.as_ref().unwrap();

            token_0.log();
            token_1.log();

            let bundle_eth_price_usd =
                price::get_eth_price_in_usd(&prices_store, pool_sqrt_price.ordinal);
            log::info!("bundle_eth_price_usd: {}", bundle_eth_price_usd);

            let token0_derived_eth_price: BigDecimal = price::find_eth_per_token(
                pool_sqrt_price.ordinal,
                &pool.address,
                &token_0.address,
                &pools_store,
                &pool_liquidities_store,
                &tokens_whitelist_pools_store,
                &total_native_value_locked_store,
                &prices_store,
            );
            log::info!(
            "token 0 {} derived eth price: {}",
            token_0.address,
            token0_derived_eth_price
        );

            let token1_derived_eth_price: BigDecimal = price::find_eth_per_token(
                pool_sqrt_price.ordinal,
                &pool.address,
                &token_1.address,
                &pools_store,
                &pool_liquidities_store,
                &tokens_whitelist_pools_store,
                &total_native_value_locked_store,
                &prices_store,
            );
            log::info!(
            "token 1 {} derived eth price: {}",
            token_1.address,
            token1_derived_eth_price
        );

            store.set(
                pool_sqrt_price.ordinal,
                keyer::bundle_eth_price(),
                &bundle_eth_price_usd,
            );

            store.set(
                pool_sqrt_price.ordinal,
                keyer::token_eth_price(&token_0.address),
                &token0_derived_eth_price,
            );

            store.set(
                pool_sqrt_price.ordinal,
                keyer::token_eth_price(&token_1.address),
                &token1_derived_eth_price,
            );
        }
    }


   fn store_total_value_locked_by_tokens(events: Events, store: StoreAddBigDecimal) {
        for event in events.events {
            log::debug!("trx_id: {}", event.transaction_id);
            let mut amount0: BigDecimal;
            let mut amount1: BigDecimal;

            match event.r#type.unwrap() {
                BurnEvent(burn) => {
                    amount0 = burn.amount_0.unwrap().into();
                    amount0 = amount0.neg();
                    amount1 = burn.amount_1.unwrap().into();
                    amount1 = amount1.neg();
                }
                MintEvent(mint) => {
                    amount0 = mint.amount_0.unwrap().into();
                    amount1 = mint.amount_1.unwrap().into();
                }
                SwapEvent(swap) => {
                    amount0 = swap.amount_0.unwrap().into();
                    amount1 = swap.amount_1.unwrap().into();
                }
            }

            store.add(
                event.log_ordinal,
                keyer::total_value_locked_by_tokens(
                    &event.pool_address,
                    &event.token0,
                    "token0".to_string(),
                ),
                &amount0,
            );
            store.add(
                event.log_ordinal,
                keyer::total_value_locked_by_tokens(
                    &event.pool_address,
                    &event.token1,
                    "token1".to_string(),
                ),
                &amount1,
            );
        }
    }


   fn store_total_value_locked(
        native_total_value_locked_deltas: store::Deltas<DeltaBigDecimal>,
        pools_store: StoreGetProto<Pool>,
        eth_prices_store: StoreGetBigDecimal,
        store: StoreSetBigDecimal,
    ) {
        // fixme: @julien: what is the use for the pool aggregator here ?
        let mut pool_aggregator: HashMap<String, (u64, BigDecimal)> = HashMap::from([]);

        // fixme: are we sure we want to unwrap and fail here ? we can't even go over the first block..
        // let eth_price_usd = helper::get_eth_price(&eth_prices_store).unwrap();

        for native_total_value_locked in native_total_value_locked_deltas.deltas {
            let eth_price_usd: BigDecimal = match &eth_prices_store.get_last(&keyer::bundle_eth_price())
            {
                None => continue,
                Some(price) => price.with_prec(100),
            };
            log::debug!(
            "eth_price_usd: {}, native_total_value_locked.key: {}",
            eth_price_usd,
            native_total_value_locked.key
        );
            if let Some(token_addr) = keyer::native_token_from_key(&native_total_value_locked.key) {
                let value = &native_total_value_locked.new_value;
                let token_derive_eth: BigDecimal =
                    match eth_prices_store.get_last(&keyer::token_eth_price(&token_addr)) {
                        None => panic!("token eth price not found for token {}", token_addr),
                        Some(price) => price,
                    };

                let total_value_locked_usd = value
                    .clone()
                    .mul(token_derive_eth)
                    .mul(eth_price_usd.clone());

                log::info!(
                "token {} total value locked usd: {}",
                token_addr,
                total_value_locked_usd
            );
                store.set(
                    native_total_value_locked.ordinal,
                    keyer::token_usd_total_value_locked(&token_addr),
                    &total_value_locked_usd,
                );
            } else if let Some((pool_addr, token_addr)) =
            keyer::native_pool_from_key(&native_total_value_locked.key)
            {
                let pool = pools_store.must_get_last(keyer::pool_key(&pool_addr));
                // we only want to use the token0
                if pool.token0.as_ref().unwrap().address != token_addr {
                    continue;
                }
                let value: BigDecimal = native_total_value_locked.new_value;
                let token_derive_eth: BigDecimal =
                    match eth_prices_store.get_last(&keyer::token_eth_price(&token_addr)) {
                        None => panic!("token eth price not found for token {}", token_addr),
                        Some(price) => price,
                    };
                let partial_pool_total_value_locked_eth = value.mul(token_derive_eth);
                log::info!(
                "partial pool {} token {} partial total value locked usd: {}",
                pool_addr,
                token_addr,
                partial_pool_total_value_locked_eth,
            );
                let aggregate_key = pool_addr.clone();

                //fixme: @julien: it seems we never actually enter here... as it would only be valid if we have
                // twice a valid event on the same pool
                if let Some(pool_agg) = pool_aggregator.get(&aggregate_key) {
                    let count = &pool_agg.0;
                    let rolling_sum = &pool_agg.1;
                    log::info!("found another partial pool value {} token {} count {} partial total value locked usd: {}",
                    pool_addr,
                    token_addr,
                    count,
                    rolling_sum,
                );
                    if count >= &(2 as u64) {
                        panic!(
                            "{}",
                            format!("this is unexpected should only see 2 pool keys")
                        )
                    }

                    log::info!(
                    "partial_pool_total_value_locked_eth: {} and rolling_sum: {}",
                    partial_pool_total_value_locked_eth,
                    rolling_sum,
                );
                    let pool_total_value_locked_eth =
                        partial_pool_total_value_locked_eth.add(rolling_sum.clone());
                    let pool_total_value_locked_usd = pool_total_value_locked_eth
                        .clone()
                        .mul(eth_price_usd.clone());
                    store.set(
                        native_total_value_locked.ordinal,
                        keyer::pool_eth_total_value_locked(&pool_addr),
                        &pool_total_value_locked_eth,
                    );
                    store.set(
                        native_total_value_locked.ordinal,
                        keyer::pool_usd_total_value_locked(&pool_addr),
                        &pool_total_value_locked_usd,
                    );

                    continue;
                }
                pool_aggregator.insert(
                    aggregate_key.clone(),
                    (1, partial_pool_total_value_locked_eth),
                );
                log::info!("partial inserted");
            }
        }
    }


   fn map_ticks(events: Events) -> Result<Ticks, Error> {
        let mut out: Ticks = Ticks { ticks: vec![] };
        for event in events.events {
            match event.r#type.unwrap() {
                BurnEvent(burn) => {
                    log::debug!("burn event transaction_id: {}", event.transaction_id);
                    let lower_tick_id: String = format!(
                        "{}#{}",
                        &event.pool_address,
                        burn.tick_lower.as_ref().unwrap().value
                    );
                    let lower_tick_idx: BigInt = burn.tick_lower.unwrap().into();
                    let lower_tick_price0 = math::big_decimal_exponated(
                        BigDecimal::try_from(1.0001).unwrap().with_prec(100),
                        lower_tick_idx.clone(),
                    );
                    let lower_tick_price1 =
                        math::safe_div(&BigDecimal::from(1 as i32), &lower_tick_price0);

                    //todo: implement the fee_growth_outside_x128_call mimicked from the smart contract
                    // to reduce the number of rpc calls to do
                    let lower_tick_result =
                        rpc::fee_growth_outside_x128_call(&event.pool_address, &lower_tick_idx);

                    let tick_lower: Tick = Tick {
                        id: lower_tick_id,
                        pool_address: event.pool_address.to_string(),
                        idx: Some(lower_tick_idx.into()),
                        price0: Some(lower_tick_price0.into()),
                        price1: Some(lower_tick_price1.into()),
                        created_at_timestamp: event.timestamp,
                        created_at_block_number: event.created_at_block_number,
                        fee_growth_outside_0x_128: Some(lower_tick_result.0.into()),
                        fee_growth_outside_1x_128: Some(lower_tick_result.1.into()),
                        log_ordinal: event.log_ordinal,
                        amount: burn.amount.clone(),
                        r#type: Lower as i32,
                        origin: Burn as i32,
                    };

                    let upper_tick_id: String = format!(
                        "{}#{}",
                        &event.pool_address,
                        burn.tick_upper.as_ref().unwrap().value
                    );
                    let upper_tick_idx: BigInt = burn.tick_upper.unwrap().into();
                    let upper_tick_price0 = math::big_decimal_exponated(
                        BigDecimal::try_from(1.0001).unwrap().with_prec(100),
                        upper_tick_idx.clone(),
                    );
                    let upper_upper_price1 =
                        math::safe_div(&BigDecimal::from(1 as i32), &upper_tick_price0);

                    let upper_tick_result =
                        rpc::fee_growth_outside_x128_call(&event.pool_address, &upper_tick_idx);

                    let tick_upper: Tick = Tick {
                        id: upper_tick_id,
                        pool_address: event.pool_address.to_string(),
                        idx: Some(upper_tick_idx.into()),
                        price0: Some(upper_tick_price0.into()),
                        price1: Some(upper_upper_price1.into()),
                        created_at_timestamp: event.timestamp,
                        created_at_block_number: event.created_at_block_number,
                        fee_growth_outside_0x_128: Some(upper_tick_result.0.into()),
                        fee_growth_outside_1x_128: Some(upper_tick_result.1.into()),
                        log_ordinal: event.log_ordinal,
                        amount: burn.amount,
                        r#type: Upper as i32,
                        origin: Burn as i32,
                    };

                    out.ticks.push(tick_lower);
                    out.ticks.push(tick_upper);
                }
                MintEvent(mint) => {
                    log::debug!("mint event transaction_id: {}", event.transaction_id);
                    let lower_tick_id: String = format!(
                        "{}#{}",
                        &event.pool_address,
                        mint.tick_lower.as_ref().unwrap().value
                    );
                    let lower_tick_idx: BigInt = mint.tick_lower.unwrap().into();
                    let lower_tick_price0 = math::big_decimal_exponated(
                        BigDecimal::try_from(1.0001).unwrap().with_prec(100),
                        lower_tick_idx.clone(),
                    );
                    let lower_tick_price1 =
                        math::safe_div(&BigDecimal::from(1 as i32), &lower_tick_price0);

                    let lower_tick_result =
                        rpc::fee_growth_outside_x128_call(&event.pool_address, &lower_tick_idx);

                    // in the subgraph, there is a `load` which is done to see if the tick
                    // exists and if it doesn't exist, createTick()
                    let tick_lower: Tick = Tick {
                        id: lower_tick_id,
                        pool_address: event.pool_address.to_string(),
                        idx: Some(lower_tick_idx.into()),
                        price0: Some(lower_tick_price0.into()),
                        price1: Some(lower_tick_price1.into()),
                        created_at_timestamp: event.timestamp,
                        created_at_block_number: event.created_at_block_number,
                        fee_growth_outside_0x_128: Some(lower_tick_result.0.into()),
                        fee_growth_outside_1x_128: Some(lower_tick_result.1.into()),
                        log_ordinal: event.log_ordinal,
                        amount: mint.amount.clone(),
                        r#type: Lower as i32,
                        origin: Mint as i32,
                    };

                    let upper_tick_id: String = format!(
                        "{}#{}",
                        &event.pool_address,
                        mint.tick_upper.as_ref().unwrap().value
                    );
                    let upper_tick_idx: BigInt = mint.tick_upper.unwrap().into();
                    let upper_tick_price0 = math::big_decimal_exponated(
                        BigDecimal::try_from(1.0001).unwrap().with_prec(100),
                        upper_tick_idx.clone(),
                    );
                    let upper_tick_price1 =
                        math::safe_div(&BigDecimal::from(1 as i32), &upper_tick_price0);

                    let upper_tick_result =
                        rpc::fee_growth_outside_x128_call(&event.pool_address, &upper_tick_idx);

                    let tick_upper: Tick = Tick {
                        id: upper_tick_id,
                        pool_address: event.pool_address.to_string(),
                        idx: Some(upper_tick_idx.into()),
                        price0: Some(upper_tick_price0.into()),
                        price1: Some(upper_tick_price1.into()),
                        created_at_timestamp: event.timestamp,
                        created_at_block_number: event.created_at_block_number,
                        fee_growth_outside_0x_128: Some(upper_tick_result.0.into()),
                        fee_growth_outside_1x_128: Some(upper_tick_result.1.into()),
                        log_ordinal: event.log_ordinal,
                        amount: mint.amount,
                        r#type: Upper as i32,
                        origin: Mint as i32,
                    };

                    out.ticks.push(tick_lower);
                    out.ticks.push(tick_upper);
                }
                _ => {}
            }
        }
        Ok(out)
    }


   fn store_ticks(ticks: Ticks, output: StoreSetProto<Tick>) {
        for tick in ticks.ticks {
            output.set(tick.log_ordinal, keyer::ticks(&tick.id), &tick);
        }
    }


   fn store_ticks_liquidities(ticks: Ticks, output: StoreAddBigInt) {
        for tick in ticks.ticks {
            log::debug!("tick id: {}", tick.id);
            if tick.origin == Mint as i32 {
                if tick.r#type == Lower as i32 {
                    output.add(
                        tick.log_ordinal,
                        keyer::tick_liquidities_net(&tick.id),
                        &BigInt::from(tick.amount.as_ref().unwrap()),
                    );
                } else {
                    // upper
                    output.add(
                        tick.log_ordinal,
                        keyer::tick_liquidities_net(&tick.id),
                        &BigInt::from(tick.amount.as_ref().unwrap()).neg(),
                    );
                }

                output.add(
                    tick.log_ordinal,
                    keyer::tick_liquidities_gross(&tick.id),
                    &BigInt::from(tick.amount.unwrap()),
                );
            } else if tick.origin == Burn as i32 {
                if tick.r#type == Lower as i32 {
                    output.add(
                        tick.log_ordinal,
                        keyer::tick_liquidities_net(&tick.id),
                        &BigInt::from(tick.amount.as_ref().unwrap()).neg(),
                    );
                } else {
                    // upper
                    output.add(
                        tick.log_ordinal,
                        keyer::tick_liquidities_net(&tick.id),
                        &BigInt::from(tick.amount.as_ref().unwrap()),
                    );
                }

                output.add(
                    tick.log_ordinal,
                    keyer::tick_liquidities_gross(&tick.id),
                    &BigInt::from(tick.amount.unwrap()).neg(),
                );
            }
        }
    }


   fn map_all_positions(
        block: Block,
        store_pool: StoreGetProto<Pool>,
    ) -> Result<Positions, Error> {
        let mut positions: Positions = Positions { positions: vec![] };

        for log in block.logs() {
            if log.address() != NON_FUNGIBLE_POSITION_MANAGER {
                continue;
            }

            if let Some(event) = abi::positionmanager::events::IncreaseLiquidity::match_and_decode(log)
            {
                if let Some(position) = utils::get_position(
                    &store_pool,
                    &Hex(log.address()).to_string(),
                    &log.receipt.transaction.hash,
                    IncreaseLiquidity,
                    log.ordinal(),
                    block
                        .header
                        .as_ref()
                        .unwrap()
                        .timestamp
                        .as_ref()
                        .unwrap()
                        .seconds as u64,
                    block.number,
                    PositionEvent {
                        event: PositionEventType::IncreaseLiquidity(event),
                    },
                ) {
                    positions.positions.push(position);
                }
            } else if let Some(event) = abi::positionmanager::events::Collect::match_and_decode(log) {
                if let Some(position) = utils::get_position(
                    &store_pool,
                    &Hex(log.address()).to_string(),
                    &log.receipt.transaction.hash,
                    Collect,
                    log.ordinal(),
                    block
                        .header
                        .as_ref()
                        .unwrap()
                        .timestamp
                        .as_ref()
                        .unwrap()
                        .seconds as u64,
                    block.number,
                    PositionEvent {
                        event: PositionEventType::Collect(event),
                    },
                ) {
                    positions.positions.push(position);
                }
            } else if let Some(event) =
            abi::positionmanager::events::DecreaseLiquidity::match_and_decode(log)
            {
                if let Some(position) = utils::get_position(
                    &store_pool,
                    &Hex(log.address()).to_string(),
                    &log.receipt.transaction.hash,
                    DecreaseLiquidity,
                    log.ordinal(),
                    block
                        .header
                        .as_ref()
                        .unwrap()
                        .timestamp
                        .as_ref()
                        .unwrap()
                        .seconds as u64,
                    block.number,
                    PositionEvent {
                        event: PositionEventType::DecreaseLiquidity(event),
                    },
                ) {
                    positions.positions.push(position);
                }
            } else if let Some(event) = abi::positionmanager::events::Transfer::match_and_decode(log) {
                if let Some(position) = utils::get_position(
                    &store_pool,
                    &Hex(log.address()).to_string(),
                    &log.receipt.transaction.hash,
                    Transfer,
                    log.ordinal(),
                    block
                        .header
                        .as_ref()
                        .unwrap()
                        .timestamp
                        .as_ref()
                        .unwrap()
                        .seconds as u64,
                    block.number,
                    PositionEvent {
                        event: PositionEventType::Transfer(event.clone()),
                    },
                ) {
                    positions.positions.push(position);
                }
            }
        }

        Ok(positions)
    }


   fn store_all_positions(positions: Positions, store: StoreSetProto<Position>) {
        for position in positions.positions {
            store.set(
                position.log_ordinal,
                keyer::all_position(
                    &position.id,
                    &PositionType::get_position_type(position.position_type).to_string(),
                ),
                &position,
            )
        }
    }


   fn map_positions(
        block: Block,
        all_positions_store: StoreGetProto<Position>,
    ) -> Result<Positions, Error> {
        let mut positions: Positions = Positions { positions: vec![] };
        let mut ordered_positions: Vec<String> = vec![];
        let mut enriched_positions: HashMap<String, Position> = HashMap::new();

        for log in block.logs() {
            if log.address() != NON_FUNGIBLE_POSITION_MANAGER {
                continue;
            }

            if let Some(event) = abi::positionmanager::events::IncreaseLiquidity::match_and_decode(log)
            {
                let token_id: String = event.token_id.to_string();
                if !enriched_positions.contains_key(&token_id) {
                    match all_positions_store.get_last(keyer::all_position(
                        &token_id,
                        &IncreaseLiquidity.to_string(),
                    )) {
                        None => {
                            log::debug!("increase liquidity for id {} doesn't exist", token_id);
                            continue;
                        }
                        Some(pos) => {
                            enriched_positions.insert(token_id.clone(), pos);
                            if !ordered_positions.contains(&String::from(token_id.clone())) {
                                ordered_positions.push(String::from(token_id))
                            }
                        }
                    }
                }
            } else if let Some(event) = abi::positionmanager::events::Collect::match_and_decode(log) {
                let token_id: String = event.token_id.to_string();
                let mut position = if !enriched_positions.contains_key(&token_id) {
                    match all_positions_store
                        .get_last(keyer::all_position(&token_id, &Collect.to_string()))
                    {
                        None => {
                            log::debug!("increase liquidity for id {} doesn't exist", token_id);
                            continue;
                        }
                        Some(pos) => pos,
                    }
                } else {
                    enriched_positions
                        .remove(&event.token_id.to_string())
                        .unwrap()
                };

                if let Some(position_call_result) =
                rpc::positions_call(&Hex(log.address()).to_string(), event.token_id)
                {
                    position.fee_growth_inside_0_last_x_128 = Some(position_call_result.5.into());
                    position.fee_growth_inside_1_last_x_128 = Some(position_call_result.6.into());
                    enriched_positions.insert(token_id.clone(), position);
                    if !ordered_positions.contains(&String::from(token_id.clone())) {
                        ordered_positions.push(String::from(token_id))
                    }
                }
            } else if let Some(event) =
            abi::positionmanager::events::DecreaseLiquidity::match_and_decode(log)
            {
                let token_id: String = event.token_id.to_string();
                if !enriched_positions.contains_key(&token_id) {
                    match all_positions_store.get_last(keyer::all_position(
                        &event.token_id.to_string(),
                        &DecreaseLiquidity.to_string(),
                    )) {
                        None => {
                            log::debug!("increase liquidity for id {} doesn't exist", token_id);
                            continue;
                        }
                        Some(pos) => {
                            enriched_positions.insert(token_id.clone(), pos);
                            if !ordered_positions.contains(&String::from(token_id.clone())) {
                                ordered_positions.push(String::from(token_id))
                            }
                        }
                    }
                }
            } else if let Some(event) = abi::positionmanager::events::Transfer::match_and_decode(log) {
                let token_id: String = event.token_id.to_string();
                let mut position = if !enriched_positions.contains_key(&token_id) {
                    match all_positions_store
                        .get_last(keyer::all_position(&token_id, &Transfer.to_string()))
                    {
                        None => {
                            log::debug!("increase liquidity for id {} doesn't exist", token_id);
                            continue;
                        }
                        Some(pos) => pos,
                    }
                } else {
                    enriched_positions.remove(&token_id).unwrap()
                };

                position.owner = Hex(event.to.as_slice()).to_string();
                enriched_positions.insert(token_id.clone(), position);
                if !ordered_positions.contains(&String::from(token_id.clone())) {
                    ordered_positions.push(String::from(token_id))
                }
            }
        }

        log::debug!("len of map: {}", enriched_positions.len());
        for element in ordered_positions.iter() {
            let pos = enriched_positions.remove(element);
            if pos.is_some() {
                positions.positions.push(pos.unwrap());
            }
        }

        Ok(positions)
    }


   fn store_position_changes(all_positions: Positions, store: StoreAddBigDecimal) {
        for position in all_positions.positions {
            match position.convert_position_type() {
                IncreaseLiquidity => {
                    store.add(
                        position.log_ordinal,
                        keyer::position_liquidity(&position.id),
                        &BigDecimal::from(position.liquidity.unwrap()),
                    );
                    store.add(
                        position.log_ordinal,
                        keyer::position_deposited_token(&position.id, "Token0"),
                        &BigDecimal::from(position.amount0.unwrap()),
                    );
                    store.add(
                        position.log_ordinal,
                        keyer::position_deposited_token(&position.id, "Token1"),
                        &BigDecimal::from(position.amount1.unwrap()),
                    );
                }
                DecreaseLiquidity => {
                    store.add(
                        position.log_ordinal,
                        keyer::position_liquidity(&position.id),
                        &BigDecimal::from(position.liquidity.unwrap()).neg(),
                    );
                    store.add(
                        position.log_ordinal,
                        keyer::position_withdrawn_token(&position.id, "Token0"),
                        &BigDecimal::from(position.amount0.unwrap()),
                    );
                    store.add(
                        position.log_ordinal,
                        keyer::position_withdrawn_token(&position.id, "Token1"),
                        &BigDecimal::from(position.amount1.unwrap()),
                    );
                }
                Collect => {
                    store.add(
                        position.log_ordinal,
                        keyer::position_collected_fees_token(&position.id, "Token0"),
                        &BigDecimal::from(position.amount0.unwrap()),
                    );
                    store.add(
                        position.log_ordinal,
                        keyer::position_collected_fees_token(&position.id, "Token1"),
                        &BigDecimal::from(position.amount1.unwrap()),
                    );
                }
                _ => {}
            }
        }
    }

//todo: maybe exact the some/none part in a macro and use it in the db?
// as in the string is empty/0 in this use-case it would mean the same thing

   fn map_position_snapshots(
        positions: Positions,
        position_changes_store: StoreGetBigDecimal,
    ) -> Result<SnapshotPositions, Error> {
        let mut snapshot_positions: SnapshotPositions = SnapshotPositions {
            snapshot_positions: vec![],
        };

        for position in positions.positions {
            let mut snapshot_position: SnapshotPosition = SnapshotPosition {
                id: format!("{}#{}", position.id, position.block_number),
                owner: position.owner,
                pool: position.pool,
                position: position.id.clone(),
                block_number: position.block_number,
                timestamp: position.timestamp,
                transaction: position.transaction,
                fee_growth_inside_0_last_x_128: position.fee_growth_inside_0_last_x_128,
                fee_growth_inside_1_last_x_128: position.fee_growth_inside_1_last_x_128,
                log_ordinal: position.log_ordinal,
                ..Default::default()
            };

            match position_changes_store.get_last(keyer::position_liquidity(&position.id)) {
                Some(liquidity) => snapshot_position.liquidity = Some(liquidity.into()),
                _ => snapshot_position.liquidity = None,
            }

            match position_changes_store
                .get_last(keyer::position_deposited_token(&position.id, "Token0"))
            {
                Some(deposited_token0) => {
                    snapshot_position.deposited_token0 = Some(deposited_token0.into());
                }
                _ => snapshot_position.deposited_token0 = Some(BigDecimal::zero().into()),
            }

            match position_changes_store
                .get_last(keyer::position_deposited_token(&position.id, "Token1"))
            {
                Some(deposited_token1) => {
                    snapshot_position.deposited_token1 = Some(deposited_token1.into());
                }
                _ => snapshot_position.deposited_token1 = Some(BigDecimal::zero().into()),
            }

            match position_changes_store
                .get_last(keyer::position_withdrawn_token(&position.id, "Token0"))
            {
                Some(withdrawn_token0) => {
                    snapshot_position.withdrawn_token0 = Some(withdrawn_token0.into());
                }
                _ => snapshot_position.withdrawn_token0 = Some(BigDecimal::zero().into()),
            }

            match position_changes_store
                .get_last(keyer::position_withdrawn_token(&position.id, "Token1"))
            {
                Some(withdrawn_token1) => {
                    snapshot_position.withdrawn_token1 = Some(withdrawn_token1.into());
                }
                _ => snapshot_position.withdrawn_token1 = Some(BigDecimal::zero().into()),
            }

            match position_changes_store
                .get_last(keyer::position_collected_fees_token(&position.id, "Token0"))
            {
                Some(collected_fees_token0) => {
                    snapshot_position.collected_fees_token0 = Some(collected_fees_token0.into());
                }
                _ => snapshot_position.collected_fees_token0 = Some(BigDecimal::zero().into()),
            }

            match position_changes_store
                .get_last(keyer::position_collected_fees_token(&position.id, "Token1"))
            {
                Some(collected_fees_token1) => {
                    snapshot_position.collected_fees_token1 = Some(collected_fees_token1.into());
                }
                _ => snapshot_position.collected_fees_token1 = Some(BigDecimal::zero().into()),
            }

            snapshot_positions
                .snapshot_positions
                .push(snapshot_position);
        }

        Ok(snapshot_positions)
    }


    fn store_swaps(map_swaps_mints_burns: pb::uniswap_types_v1::Events, store: StoreSetProto<pb::uniswap_types_v1::Swap>) {
        todo!()
    }


    fn map_fees(block: Block) -> Result<pb::uniswap_types_v1::Fees, Error> {
        todo!()
    }


    fn store_fees(block: Block, store: StoreSetProto<pb::uniswap_types_v1::Fee>) {
        todo!()
    }


    fn map_flashes(block: Block, pool_store: StoreGetProto<Pool>) -> Result<Flashes, Error> {
         let mut out = Flashes { flashes: vec![] };

         for log in block.logs() {
             if abi::pool::events::Flash::match_log(&log.log) {
                 let pool_address: String = Hex(&log.address()).to_string();

                 match pool_store.get_last(keyer::pool_key(&pool_address)) {
                     None => {
                         panic!("pool {} not found for flash", pool_address)
                     }
                     Some(_) => {
                         log::info!("pool_address: {}", pool_address);
                         let (fee_growth_global_0x_128, fee_growth_global_1x_128) =
                             rpc::fee_growth_global_x128_call(&pool_address);

                         out.flashes.push(Flash {
                             pool_address,
                             fee_growth_global_0x_128: Some(fee_growth_global_0x_128.into()),
                             fee_growth_global_1x_128: Some(fee_growth_global_1x_128.into()),
                             log_ordinal: log.ordinal(),
                         });
                     }
                 }
             }
         }

         Ok(out)
     }

//todo: when a pool is created, we also save the token
// (id, name, symbol, decimals and total supply)
// issue here is what if we have multiple pools with t1-t2, t1-t3, t1-t4, etc.
// we will have t1 generate multiple entity changes for nothing since it has
// already been emitted -- subgraph doesn't solve this either (Optimization)

    fn map_bundle_entities(
         block: Block,
         derived_eth_prices_deltas: store::Deltas<DeltaBigDecimal>,
     ) -> Result<EntityChanges, Error> {
         let mut entity_changes: EntityChanges = Default::default();

         if block.number == 12369621 {
             db::created_bundle_entity_change(&mut entity_changes);
         }

         db::bundle_store_eth_price_usd_bundle_entity_change(
             &mut entity_changes,
             derived_eth_prices_deltas,
         );

         Ok(entity_changes)
     }


    fn map_factory_entities(
         block: Block,
         pool_count_deltas: store::Deltas<DeltaBigInt>,
         tx_count_deltas: store::Deltas<DeltaBigInt>,
         swaps_volume_deltas: store::Deltas<DeltaBigDecimal>,
         totals_deltas: store::Deltas<DeltaBigDecimal>,
     ) -> Result<EntityChanges, Error> {
         let mut entity_changes: EntityChanges = Default::default();

         // FIXME: Hard-coded start block, how could we pull that from the manifest?
         if block.number == 12369621 {
             db::factory_created_factory_entity_change(&mut entity_changes)
         }

         db::pool_created_factory_entity_change(&mut entity_changes, pool_count_deltas);
         db::tx_count_factory_entity_change(&mut entity_changes, tx_count_deltas);
         db::swap_volume_factory_entity_change(&mut entity_changes, swaps_volume_deltas);
         db::total_value_locked_factory_entity_change(&mut entity_changes, totals_deltas);

         Ok(entity_changes)
     }


    fn map_pool_entities(
         pools_created: Pools,
         pool_sqrt_price_deltas: store::Deltas<DeltaProto<PoolSqrtPrice>>,
         pool_liquidities_store_deltas: store::Deltas<DeltaBigInt>,
         total_value_locked_deltas: store::Deltas<DeltaBigDecimal>,
         total_value_locked_by_tokens_deltas: store::Deltas<DeltaBigDecimal>,
         pool_fee_growth_global_x128_deltas: store::Deltas<DeltaBigInt>,
         price_deltas: store::Deltas<DeltaBigDecimal>,
         tx_count_deltas: store::Deltas<DeltaBigInt>,
         swaps_volume_deltas: store::Deltas<DeltaBigDecimal>,
     ) -> Result<EntityChanges, Error> {
         let mut entity_changes: EntityChanges = Default::default();
         db::pools_created_pool_entity_change(pools_created, &mut entity_changes);
         db::pool_sqrt_price_entity_change(&mut entity_changes, pool_sqrt_price_deltas);
         db::pool_liquidities_pool_entity_change(&mut entity_changes, pool_liquidities_store_deltas);
         db::total_value_locked_pool_entity_change(&mut entity_changes, total_value_locked_deltas);
         db::total_value_locked_by_token_pool_entity_change(
             &mut entity_changes,
             total_value_locked_by_tokens_deltas,
         );
         db::pool_fee_growth_global_x128_entity_change(
             &mut entity_changes,
             pool_fee_growth_global_x128_deltas,
         );
         db::price_pool_entity_change(&mut entity_changes, price_deltas);
         db::tx_count_pool_entity_change(&mut entity_changes, tx_count_deltas);
         db::swap_volume_pool_entity_change(&mut entity_changes, swaps_volume_deltas);
         Ok(entity_changes)
     }


    fn map_tokens_entities(
         pools_created: Pools,
         swaps_volume_deltas: store::Deltas<DeltaBigDecimal>,
         tx_count_deltas: store::Deltas<DeltaBigInt>,
         total_value_locked_by_deltas: store::Deltas<DeltaBigDecimal>,
         total_value_locked_deltas: store::Deltas<DeltaBigDecimal>,
         derived_eth_prices_deltas: store::Deltas<DeltaBigDecimal>,
         tokens_whitelist_pools: store::Deltas<DeltaArray<String>>,
     ) -> Result<EntityChanges, Error> {
         let mut entity_changes: EntityChanges = Default::default();
         db::tokens_created_token_entity_change(&mut entity_changes, pools_created);
         db::swap_volume_token_entity_change(&mut entity_changes, swaps_volume_deltas);
         db::tx_count_token_entity_change(&mut entity_changes, tx_count_deltas);
         db::total_value_locked_by_token_token_entity_change(
             &mut entity_changes,
             total_value_locked_by_deltas,
         );
         db::total_value_locked_usd_token_entity_change(&mut entity_changes, total_value_locked_deltas);
         db::derived_eth_prices_token_entity_change(&mut entity_changes, derived_eth_prices_deltas);
         db::whitelist_token_entity_change(&mut entity_changes, tokens_whitelist_pools);

         Ok(entity_changes)
     }


    fn map_tick_entities(
         ticks_deltas: store::Deltas<DeltaProto<Tick>>,
         ticks_liquidities_deltas: store::Deltas<DeltaBigInt>,
     ) -> Result<EntityChanges, Error> {
         let mut entity_changes: EntityChanges = Default::default();
         db::create_or_update_ticks_entity_change(&mut entity_changes, ticks_deltas);
         db::ticks_liquidities_tick_entity_change(&mut entity_changes, ticks_liquidities_deltas);
         Ok(entity_changes)
     }


    fn map_position_entities(
         positions: Positions,
         positions_changes_deltas: store::Deltas<DeltaBigDecimal>,
     ) -> Result<EntityChanges, Error> {
         let mut entity_changes: EntityChanges = Default::default();
         db::position_create_entity_change(positions, &mut entity_changes);
         db::positions_changes_entity_change(&mut entity_changes, positions_changes_deltas);
         Ok(entity_changes)
     }


    fn map_position_snapshot_entities(
         snapshot_positions: SnapshotPositions,
     ) -> Result<EntityChanges, Error> {
         let mut entity_changes: EntityChanges = Default::default();
         db::snapshot_position_entity_change(snapshot_positions, &mut entity_changes);
         Ok(entity_changes)
     }


    fn map_transaction_entities(transactions: Transactions) -> Result<EntityChanges, Error> {
         let mut entity_changes: EntityChanges = Default::default();
         db::transaction_entity_change(transactions, &mut entity_changes);
         Ok(entity_changes)
     }

//todo: check if we want to check the block ordinal here and sort by the ordinal
// or simply stream out all the entity changes

    fn map_swaps_mints_burns_entities(
         events: Events,
         tx_count_store: StoreGetBigInt,
         store_eth_prices: StoreGetBigDecimal,
     ) -> Result<EntityChanges, Error> {
         let mut entity_changes: EntityChanges = Default::default();
         db::swaps_mints_burns_created_entity_change(
             events,
             tx_count_store,
             store_eth_prices,
             &mut entity_changes,
         );
         Ok(entity_changes)
     }


    fn map_flash_entities(flashes: Flashes) -> Result<EntityChanges, Error> {
         let mut entity_changes: EntityChanges = Default::default();
         db::flashes_update_pool_fee_entity_change(flashes, &mut entity_changes);
         Ok(entity_changes)
     }

    fn map_uniswap_day_data_entities(
         tx_count_deltas: store::Deltas<DeltaBigInt>,
         totals_deltas: store::Deltas<DeltaBigDecimal>,
         volume_deltas: store::Deltas<DeltaBigDecimal>,
     ) -> Result<EntityChanges, Error> {
         let mut entity_changes: EntityChanges = Default::default();
         db::uniswap_day_data_tx_count_entity_change(&mut entity_changes, tx_count_deltas);
         db::uniswap_day_data_totals_entity_change(&mut entity_changes, totals_deltas);
         db::uniswap_day_data_volumes_entity_change(&mut entity_changes, volume_deltas);
         Ok(entity_changes)
     }

    fn graph_out(
         factory_entities: EntityChanges,
         bundle_entities: EntityChanges,
         transaction_entities: EntityChanges,
         pool_entities: EntityChanges,
         token_entities: EntityChanges,
         tick_entities: EntityChanges,
         position_entities: EntityChanges,
         position_snapshot_entities: EntityChanges,
         flash_entities: EntityChanges,
         swaps_mints_burns_entities: EntityChanges,
     ) -> Result<EntityChanges, Error> {
         Ok(EntityChanges {
             entity_changes: [
                 factory_entities.entity_changes,
                 bundle_entities.entity_changes,
                 transaction_entities.entity_changes,
                 pool_entities.entity_changes,
                 token_entities.entity_changes,
                 tick_entities.entity_changes,
                 position_entities.entity_changes,
                 position_snapshot_entities.entity_changes,
                 flash_entities.entity_changes,
                 swaps_mints_burns_entities.entity_changes,
             ]
                 .concat(),
         })
     }

    fn dummy_graph_out_store(graph_out: EntityChanges, store: StoreSetString) {
        todo!()
    }
}