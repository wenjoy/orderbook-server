pub mod book;
pub mod order;

use book::OrderBook;
use order::{Order, OrderBookSnapshot, OrderStatus, Side, Trade};
use uuid::Uuid;

use crate::engine::order::OrderStatus::Open;

#[derive(Debug)]
pub enum EngineError {
    OrderNotFound(uuid::Uuid),
    InvalidOrder(String),
    InSufficientLiquidity,
    OrderAlreadyClosed,
}

pub type EngineResult<T> = std::result::Result<T, EngineError>;

#[derive(Clone, Default)]
pub struct MatchingEngine {
    pub book: OrderBook,
    pub trades: Vec<Trade>,
}

impl MatchingEngine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn place_order(&mut self, mut order: Order) -> EngineResult<(Order, Vec<Trade>)> {
        if order.quantity == 0 {
            return Err(EngineError::InvalidOrder(String::from(
                "quantity must be > 0",
            )));
        }

        let mut new_trades: Vec<Trade> = Vec::new();

        match order.side {
            Side::Ask => self.match_ask(&mut order, &mut new_trades),
            Side::Bid => self.match_bid(&mut order, &mut new_trades),
        }

        if order.remaining() > 0 {
            order.status = if order.filled_quantity > 0 {
                OrderStatus::PartiallyFilled
            } else {
                OrderStatus::Open
            };
            // println!("add order {}", order.price.unwrap());
            self.book.add_order_to_book(&order);
        } else if order.remaining() == 0 {
            order.status = OrderStatus::Filled;
        } else {
            order.status = OrderStatus::PartiallyFilled;
        }

        // dbg!(&order);
        self.trades.extend(new_trades.clone());
        Ok((order, new_trades))
    }

    pub fn open_orders(&self, user_filter: Option<&str>) -> Vec<Order> {
        let mut out = Vec::new();
        println!("debug {}", self.book.bids.len() + self.book.asks.len());
        for (_, price_level) in self.book.asks.iter().chain(self.book.bids.iter()) {
            for order in price_level.orders.iter() {
                if order.status != Open {
                    continue;
                }

                if let Some(u) = user_filter {
                    if u == order.user {
                        out.push(order.clone());
                    }
                } else {
                    out.push(order.clone());
                };
            }
        }
        out
    }

    fn match_ask(&mut self, taker: &mut Order, trades: &mut Vec<Trade>) {
        while taker.remaining() > 0 {
            let Some(best_bid_price) = self.book.best_bid() else {
                break;
            };
            if let Some(limit_price) = taker.price
                && limit_price > best_bid_price
            {
                break;
            }

            let price_level = self
                .book
                .bids
                .get_mut(&best_bid_price)
                .expect("not a valid bid price");

            for maker in price_level.orders.iter_mut() {
                let traded = maker.remaining().min(taker.remaining());

                if traded == 0 {
                    continue;
                }

                trades.push(Trade {
                    maker_order_id: maker.id,
                    taker_order_id: taker.id,
                    price: best_bid_price,
                    side: Side::Ask,
                    quantity: traded,
                    id: Uuid::new_v4(),
                    timestamp: chrono::Utc::now(),
                    taker_user: taker.user.clone(),
                    maker_user: maker.user.clone(),
                    market: taker.market.clone(),
                });
                taker.filled_quantity += traded;
                maker.filled_quantity += traded;

                if maker.remaining() == 0 {
                    maker.status = OrderStatus::Filled;
                    // fully_filled_ids.push(maker.id);
                } else {
                    maker.status = OrderStatus::PartiallyFilled;
                }

                if taker.remaining() == 0 {
                    taker.status = OrderStatus::Filled;
                    break;
                }
            }

            if price_level.orders.iter().all(|o| o.remaining() == 0) {
                self.book.bids.remove(&best_bid_price);
            } else {
                price_level.orders.retain(|o| o.remaining() > 0);
            }
        }

        if taker.filled_quantity > 0 && taker.remaining() > 0 {
            taker.status = OrderStatus::PartiallyFilled;
        }
    }
    fn match_bid(&mut self, taker: &mut Order, trades: &mut Vec<Trade>) {
        // dbg!(&taker.status);

        while taker.remaining() > 0 {
            let Some(best_ask_price) = self.book.best_ask() else {
                break;
            };

            if let Some(limit_price) = taker.price
                && limit_price < best_ask_price
            {
                break;
            }

            let price_level = self
                .book
                .asks
                .get_mut(&best_ask_price)
                .expect("not a valid price");

            for maker in price_level.orders.iter_mut() {
                let traded = maker.remaining().min(taker.remaining());

                trades.push(Trade {
                    maker_order_id: maker.id,
                    taker_order_id: taker.id,
                    price: best_ask_price,
                    side: Side::Bid,
                    quantity: traded,
                    id: Uuid::new_v4(),
                    timestamp: chrono::Utc::now(),
                    taker_user: taker.user.clone(),
                    maker_user: maker.user.clone(),
                    market: taker.market.clone(),
                });
                taker.filled_quantity += traded;
                maker.filled_quantity += traded;

                if taker.remaining() == 0 {
                    break;
                }
            }

            if price_level.orders.iter().all(|o| o.remaining() == 0) {
                self.book.asks.remove(&best_ask_price);
            } else {
                price_level.orders.retain(|o| o.remaining() > 0);
            }
        }
    }

    pub fn get_snapshot(&self, depth: usize) -> OrderBookSnapshot {
        self.book.snapshot(depth)
    }

    pub fn cancel_order(&mut self, order_id: Uuid) -> EngineResult<Order> {
        let mut order = self
            .book
            .remove_order(order_id)
            .ok_or(EngineError::OrderNotFound(order_id))?;

        if !order.is_active() {
            return Err(EngineError::OrderAlreadyClosed);
        }

        order.status = OrderStatus::Cancelled;
        Ok(order)
    }
}
