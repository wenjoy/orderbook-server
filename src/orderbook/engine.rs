use crate::{
    error::{EngineError, Result},
    orderbook::book::OrderBook,
    types::{Order, OrderStatus, OrderType, Side, Trade},
};

#[derive(Clone, Default)]
pub struct MatchingEngine {
    book: OrderBook,
    trades: Vec<Trade>,
}

impl MatchingEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn place_order(&mut self, mut order: Order) -> Result<(Order, Vec<Trade>)> {
        if order.quantity == 0 {
            return Err(EngineError::InvalidOrder(String::from(
                "quantity must be > 0",
            )));
        }

        if matches!(order.order_type, OrderType::Limit) && order.price.is_none() {
            return Err(EngineError::InvalidOrder(
                "limit order requires price".into(),
            ));
        }

        let mut new_trades: Vec<Trade> = Vec::new();

        match order.side {
            Side::Ask => self.match_ask(),
            Side::Bid => self.match_bid(),
        }

        if order.remaining() > 0 && matches!(order.order_type, OrderType::Limit) {
            order.status = if order.filled_quantity > 0 {
                OrderStatus::PartiallyFilled
            } else {
                OrderStatus::Open
            };
            self.book.add_order();
        } else if order.remaining() == 0 {
            order.status = OrderStatus::Filled;
        } else {
            order.status = OrderStatus::PartiallyFilled;
        }

        self.trades.extend(new_trades.clone());
        Ok((order, new_trades))
    }

    fn match_ask(&self) {
        todo!()
    }
    fn match_bid(&self) {
        todo!()
    }
}
