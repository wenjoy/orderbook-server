use std::collections::{BTreeMap, VecDeque};

use uuid::Uuid;

use crate::types::{Order, Side};

#[derive(Clone, Default)]
pub struct PriceLevel {
    pub orders: VecDeque<Order>,
}
#[derive(Clone, Default)]
pub struct OrderBook {
    pub bids: BTreeMap<u64, PriceLevel>,
    pub asks: BTreeMap<u64, PriceLevel>,
    pub order_index: std::collections::HashMap<Uuid, (Side, u64)>, // order_id -> (side, price)
    pub sequence: u64,
}
impl OrderBook {
    pub fn best_ask(&self) -> Option<u64> {
        self.asks.keys().next().copied()
    }
    pub fn best_bid(&self) -> Option<u64> {
        self.bids.keys().next_back().copied()
    }
}

impl OrderBook {
    pub fn add_order(&mut self, order: &Order) {
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
}
