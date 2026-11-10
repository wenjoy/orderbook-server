use std::{
    collections::{BTreeMap, VecDeque},
    default,
};

use uuid::Uuid;

use super::order::{DepthLevel, Order, OrderBookSnapshot, Side};

#[derive(Clone, Default)]
pub struct PriceLevel {
    pub orders: VecDeque<Order>,
}
impl PriceLevel {
    pub fn total_quantity(&self) -> u64 {
        self.orders.iter().map(|o| o.remaining()).sum()
    }
    pub fn is_empty(&self) -> bool {
        self.orders.iter().all(|o| o.remaining() == 0)
    }
}
#[derive(Clone, Default)]
pub struct OrderBook {
    pub bids: BTreeMap<u64, PriceLevel>,
    pub asks: BTreeMap<u64, PriceLevel>,
    pub order_index: std::collections::HashMap<Uuid, (Side, u64)>, // order_id -> (side, price)
    pub sequence: u64,
    //TODO: next_order_id
}

impl OrderBook {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn best_ask(&self) -> Option<u64> {
        self.asks.keys().next().copied()
    }
    pub fn best_bid(&self) -> Option<u64> {
        self.bids.keys().next_back().copied()
    }
    pub fn bid_depth(&self, depth: usize) -> Vec<DepthLevel> {
        self.bids
            .iter()
            .rev()
            .take(depth)
            .map(|(price, price_level)| DepthLevel {
                price: *price,
                quantity: price_level.orders.iter().map(|o| o.quantity).sum(),
            })
            .collect()
    }
    pub fn ask_depth(&self, depth: usize) -> Vec<DepthLevel> {
        self.asks
            .iter()
            .take(depth)
            .map(|(price, price_level)| DepthLevel {
                price: *price,
                quantity: price_level.orders.iter().map(|o| o.quantity).sum(),
            })
            .collect()
    }
    pub fn add_order_to_book(&mut self, order: &Order) {
        let price = order.price.expect("limit order must have price");
        let book = match order.side {
            Side::Bid => &mut self.bids,
            Side::Ask => &mut self.asks,
        };

        book.entry(price)
            .or_default()
            .orders
            .push_back(order.clone());

        self.order_index
            .insert(order.id, (order.side.clone(), price));
        self.sequence += 1;
    }

    pub fn remove_order(&mut self, order_id: Uuid) -> Option<Order> {
        let (side, price) = self.order_index.remove(&order_id)?;

        let book = match side {
            Side::Ask => &mut self.asks,
            Side::Bid => &mut self.bids,
        };

        if let Some(level) = book.get_mut(&price) {
            let pos = level.orders.iter().position(|o| o.id == order_id).unwrap();
            let order = level.orders.remove(pos).unwrap();

            if level.orders.is_empty() {
                book.remove(&price);
            }

            self.sequence += 1;
            return Some(order);
        }
        None
    }

    pub fn snapshot(&self, depth: usize) -> OrderBookSnapshot {
        let bids = self
            .bids
            .iter()
            .rev()
            .take(depth)
            .map(|(&price, level)| DepthLevel {
                price,
                quantity: level.total_quantity(),
            })
            .collect();

        let asks = self
            .asks
            .iter()
            .take(depth)
            .map(|(&price, level)| DepthLevel {
                price,
                quantity: level.total_quantity(),
            })
            .collect();

        OrderBookSnapshot {
            bids,
            asks,
            sequence: self.sequence,
        }
    }
}
