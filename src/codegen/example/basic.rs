//! API/SPI setup, authentication, market data, and an optional single order.
//!
//! Run with `cargo run --example basic` after setting these environment variables:
//! CTP_TRADER_FRONT, CTP_MD_FRONT, CTP_BROKER_ID, CTP_USER_ID, CTP_PASSWORD,
//! CTP_AUTH_CODE, CTP_APP_ID, and CTP_INSTRUMENT.
//!
//! By default this example only logs in and listens; it never sends orders.
//! To send ONE BUY/OPEN limit order, also set CTP_ENABLE_ORDERS=1,
//! CTP_EXCHANGE_ID, CTP_ORDER_PRICE, and optionally CTP_ORDER_VOLUME (default: 1).
//! The order is cancelled if it is still queued when its order report arrives.
//! It may fill before cancellation. This example does NOT automatically close positions.
//!
//! CTP_FLOW_PATH defaults to ./flow/basic; CTP_RUN_SECONDS defaults to 60.
//! Use a simulation account first. This is an API example, not a complete trading system.

use std::{
    env,
    error::Error,
    path::PathBuf,
    sync::mpsc::{self, Sender},
    time::{Duration, Instant},
};

use __CRATE_NAME__::{
    decode_ctp_text, InputOrder, InputOrderAction, MdApi, MdSpi, ReqAuthenticate, ReqUserLogin,
    SettlementInfoConfirm, TraderApi, TraderSpi, sys,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;

#[derive(Default)]
struct RequestIds {
    last: i32,
}

impl RequestIds {
    fn next(&mut self) -> Result<i32> {
        self.last = self.last.checked_add(1).ok_or("request ID exhausted")?;
        Ok(self.last)
    }
}

struct Config {
    trader_front: String,
    md_front: String,
    broker: String,
    user: String,
    password: String,
    auth_code: String,
    app_id: String,
    instrument: String,
    flow_path: PathBuf,
    run_seconds: u64,
    order: Option<OrderConfig>,
}

struct OrderConfig {
    exchange: String,
    price: f64,
    volume: i32,
}

impl Config {
    fn from_env() -> Result<Self> {
        let order = if env::var("CTP_ENABLE_ORDERS").as_deref() == Ok("1") {
            let price: f64 = env::var("CTP_ORDER_PRICE")?.parse()?;
            let volume: i32 = env::var("CTP_ORDER_VOLUME")
                .unwrap_or_else(|_| "1".into())
                .parse()?;
            if !price.is_finite() || price <= 0.0 || volume <= 0 {
                return Err("order price and volume must be positive; price must be finite".into());
            }
            Some(OrderConfig {
                exchange: env::var("CTP_EXCHANGE_ID")?,
                price,
                volume,
            })
        } else {
            None
        };
        Ok(Self {
            trader_front: env::var("CTP_TRADER_FRONT")?,
            md_front: env::var("CTP_MD_FRONT")?,
            broker: env::var("CTP_BROKER_ID")?,
            user: env::var("CTP_USER_ID")?,
            password: env::var("CTP_PASSWORD")?,
            auth_code: env::var("CTP_AUTH_CODE")?,
            app_id: env::var("CTP_APP_ID")?,
            instrument: env::var("CTP_INSTRUMENT")?,
            flow_path: env::var_os("CTP_FLOW_PATH")
                .map(PathBuf::from)
                .unwrap_or_else(|| "./flow/basic".into()),
            run_seconds: env::var("CTP_RUN_SECONDS")
                .unwrap_or_else(|_| "60".into())
                .parse()?,
            order,
        })
    }

    fn login_request(&self) -> Result<sys::CThostFtdcReqUserLoginField> {
        Ok(ReqUserLogin::builder()
            .broker_id(&self.broker)
            .user_id(&self.user)
            .password(&self.password)
            .build()?)
    }
}

// Only owned data crosses the channel: SDK pointers are valid just for the callback.
enum Event {
    TraderConnected,
    MdConnected,
    Disconnected {
        api: &'static str,
        reason: i32,
    },
    Authenticated,
    TraderLoggedIn {
        day: String,
        max_order_ref: String,
    },
    MdLoggedIn,
    SettlementConfirmed,
    Subscribed(String),
    Quote {
        instrument: String,
        price: f64,
        lower: f64,
        upper: f64,
    },
    Order(Box<sys::CThostFtdcOrderField>),
    Trade(Box<sys::CThostFtdcTradeField>),
    BusinessError {
        callback: &'static str,
        code: i32,
    },
    MissingResponse(&'static str),
}

// Identifiers are ASCII C strings. Do not use this helper to assume ErrorMsg is UTF-8;
// SDK business messages may use GBK. business_message() uses the decoded error.xml instead.
fn text(value: &[std::ffi::c_char]) -> String { decode_ctp_text(value) }

fn response_ok(
    sender: &Sender<Event>,
    callback: &'static str,
    info: Option<&sys::CThostFtdcRspInfoField>,
) -> bool {
    if let Some(info) = info.filter(|info| info.ErrorID != 0) {
        let _ = sender.send(Event::BusinessError {
            callback,
            code: info.ErrorID,
        });
        false
    } else {
        true
    }
}

fn business_message(code: i32) -> String {
    __BUSINESS_MESSAGE__
}

struct TraderHandler {
    sender: Sender<Event>,
}

impl TraderSpi for TraderHandler {
    fn on_front_connected(&self) {
        let _ = self.sender.send(Event::TraderConnected);
    }

    fn on_front_disconnected(&self, reason: i32) {
        let _ = self.sender.send(Event::Disconnected {
            api: "Trader",
            reason,
        });
    }

    fn on_rsp_authenticate(
        &self,
        _: Option<&sys::CThostFtdcRspAuthenticateField>,
        info: Option<&sys::CThostFtdcRspInfoField>,
        _: i32,
        last: bool,
    ) {
        if response_ok(&self.sender, "on_rsp_authenticate", info) && last {
            let _ = self.sender.send(Event::Authenticated);
        }
    }

    fn on_rsp_user_login(
        &self,
        response: Option<&sys::CThostFtdcRspUserLoginField>,
        info: Option<&sys::CThostFtdcRspInfoField>,
        _: i32,
        last: bool,
    ) {
        if response_ok(&self.sender, "trader on_rsp_user_login", info) && last {
            let event = match response {
                Some(response) => Event::TraderLoggedIn {
                    day: text(&response.TradingDay),
                    max_order_ref: text(&response.MaxOrderRef),
                },
                None => Event::MissingResponse("trader on_rsp_user_login"),
            };
            let _ = self.sender.send(event);
        }
    }

    fn on_rsp_settlement_info_confirm(
        &self,
        _: Option<&sys::CThostFtdcSettlementInfoConfirmField>,
        info: Option<&sys::CThostFtdcRspInfoField>,
        _: i32,
        last: bool,
    ) {
        if response_ok(&self.sender, "on_rsp_settlement_info_confirm", info) && last {
            let _ = self.sender.send(Event::SettlementConfirmed);
        }
    }

    fn on_rtn_order(&self, order: Option<&sys::CThostFtdcOrderField>) {
        if let Some(order) = order {
            let _ = self.sender.send(Event::Order(Box::new(*order)));
        }
    }

    fn on_rtn_trade(&self, trade: Option<&sys::CThostFtdcTradeField>) {
        if let Some(trade) = trade {
            let _ = self.sender.send(Event::Trade(Box::new(*trade)));
        }
    }

    fn on_rsp_order_insert(
        &self,
        _: Option<&sys::CThostFtdcInputOrderField>,
        info: Option<&sys::CThostFtdcRspInfoField>,
        _: i32,
        _: bool,
    ) {
        response_ok(&self.sender, "on_rsp_order_insert", info);
    }

    fn on_err_rtn_order_insert(
        &self,
        _: Option<&sys::CThostFtdcInputOrderField>,
        info: Option<&sys::CThostFtdcRspInfoField>,
    ) {
        response_ok(&self.sender, "on_err_rtn_order_insert", info);
    }

    fn on_rsp_order_action(
        &self,
        _: Option<&sys::CThostFtdcInputOrderActionField>,
        info: Option<&sys::CThostFtdcRspInfoField>,
        _: i32,
        _: bool,
    ) {
        response_ok(&self.sender, "on_rsp_order_action", info);
    }

    fn on_err_rtn_order_action(
        &self,
        _: Option<&sys::CThostFtdcOrderActionField>,
        info: Option<&sys::CThostFtdcRspInfoField>,
    ) {
        response_ok(&self.sender, "on_err_rtn_order_action", info);
    }

    fn on_rsp_error(&self, info: Option<&sys::CThostFtdcRspInfoField>, _: i32, _: bool) {
        response_ok(&self.sender, "trader on_rsp_error", info);
    }
}

struct MdHandler {
    sender: Sender<Event>,
}

impl MdSpi for MdHandler {
    fn on_front_connected(&self) {
        let _ = self.sender.send(Event::MdConnected);
    }

    fn on_front_disconnected(&self, reason: i32) {
        let _ = self.sender.send(Event::Disconnected { api: "MD", reason });
    }

    fn on_rsp_user_login(
        &self,
        _: Option<&sys::CThostFtdcRspUserLoginField>,
        info: Option<&sys::CThostFtdcRspInfoField>,
        _: i32,
        last: bool,
    ) {
        if response_ok(&self.sender, "md on_rsp_user_login", info) && last {
            let _ = self.sender.send(Event::MdLoggedIn);
        }
    }

    fn on_rsp_sub_market_data(
        &self,
        response: Option<&sys::CThostFtdcSpecificInstrumentField>,
        info: Option<&sys::CThostFtdcRspInfoField>,
        _: i32,
        _: bool,
    ) {
        if response_ok(&self.sender, "on_rsp_sub_market_data", info) {
            if let Some(response) = response {
                let _ = self
                    .sender
                    .send(Event::Subscribed(text(&response.InstrumentID)));
            }
        }
    }

    fn on_rtn_depth_market_data(&self, data: Option<&sys::CThostFtdcDepthMarketDataField>) {
        if let Some(data) = data {
            let _ = self.sender.send(Event::Quote {
                instrument: text(&data.InstrumentID),
                price: data.LastPrice,
                lower: data.LowerLimitPrice,
                upper: data.UpperLimitPrice,
            });
        }
    }

    fn on_rsp_error(&self, info: Option<&sys::CThostFtdcRspInfoField>, _: i32, _: bool) {
        response_ok(&self.sender, "md on_rsp_error", info);
    }
}

fn main() -> Result<()> {
    let config = Config::from_env()?;
    let deadline = Instant::now()
        .checked_add(Duration::from_secs(config.run_seconds))
        .ok_or("CTP_RUN_SECONDS is too large")?;
    let trader_path = config.flow_path.join("trader");
    let md_path = config.flow_path.join("md");
    std::fs::create_dir_all(&trader_path)?;
    std::fs::create_dir_all(&md_path)?;
    let (sender, receiver) = mpsc::channel();

    // Create the channel and all handler state before build(): callbacks can start immediately.
    // The SDK's default production mode also applies to ordinary SimNow front addresses.
    let trader = TraderApi::builder()
        .flow_path(trader_path)
        .register_front(&config.trader_front)
        .subscribe_private_topic(__PRIVATE_TOPIC_ARGS__)
        .subscribe_public_topic(sys::THOST_TE_RESUME_TYPE_THOST_TERT_QUICK)
        .spi(TraderHandler {
            sender: sender.clone(),
        })
        .build()?;
    let md = MdApi::builder()
        .flow_path(md_path)
        .register_front(&config.md_front)
        .spi(MdHandler { sender })
        .build()?;
    println!(
        "Trader SDK: {}; MD SDK: {}",
        TraderApi::api_version(),
        MdApi::api_version()
    );
    println!("Orders enabled: {}", config.order.is_some());

    let mut trader_ready = false;
    let mut md_ready = false;
    let mut quote = None;
    let mut next_reference = String::new();
    let mut submitted_reference: Option<String> = None;
    let mut cancel_sent = false;
    // Allocate IDs in send order, starting at 1. Keep this counter across reconnects.
    // All requests are sent from this thread, so no atomic counter or lock is needed.
    let mut request_ids = RequestIds::default();

    // Only this thread calls the APIs. Callbacks quickly forward owned events without blocking
    // on business processing. The unbounded channel is for this small example, not a queue policy
    // suitable for unlimited market data or slow consumers in a production system.
    while let Some(remaining) = deadline.checked_duration_since(Instant::now()) {
        let event = match receiver.recv_timeout(remaining) {
            Ok(event) => event,
            Err(mpsc::RecvTimeoutError::Timeout) => break,
            Err(error) => return Err(error.into()),
        };
        match event {
            Event::TraderConnected => {
                trader_ready = false;
                let mut request = ReqAuthenticate::builder()
                    .broker_id(&config.broker)
                    .user_id(&config.user)
                    .auth_code(&config.auth_code)
                    .app_id(&config.app_id)
                    .build()?;
                let request_id = request_ids.next()?;
                println!("Trader connected; sending authentication, request ID: {request_id}");
                trader.req_authenticate(&mut request, request_id)?;
            }
            Event::Authenticated => {
                let request_id = request_ids.next()?;
                println!("Trader authenticated; sending login, request ID: {request_id}");
                trader.req_user_login(&mut config.login_request()?, request_id)?;
            }
            Event::TraderLoggedIn { day, max_order_ref } => {
                println!("Trader logged in; trading day: {day}");
                let max_reference = if max_order_ref.trim().is_empty() {
                    0
                } else {
                    max_order_ref.trim().parse::<u64>()?
                };
                next_reference = max_reference
                    .checked_add(1)
                    .ok_or("order reference overflow")?
                    .to_string();
                if config.order.is_some() {
                    // Only the opt-in order demonstration confirms settlement.
                    let mut request = SettlementInfoConfirm::builder()
                        .broker_id(&config.broker)
                        .investor_id(&config.user)
                        .build()?;
                    let request_id = request_ids.next()?;
                    println!("Sending settlement confirmation, request ID: {request_id}");
                    trader.req_settlement_info_confirm(&mut request, request_id)?;
                }
            }
            Event::SettlementConfirmed => {
                println!("Settlement confirmed; Trader is ready for the optional order");
                trader_ready = true;
            }
            Event::MdConnected => {
                md_ready = false;
                quote = None;
                let request_id = request_ids.next()?;
                println!("MD connected; sending login, request ID: {request_id}");
                md.req_user_login(&mut config.login_request()?, request_id)?;
            }
            Event::MdLoggedIn => {
                println!("MD logged in; subscribing to {}", config.instrument);
                md.subscribe_market_data(&[&config.instrument])?;
                md_ready = true;
            }
            Event::Subscribed(instrument) => println!("Subscription accepted: {instrument}"),
            Event::Quote {
                instrument,
                price,
                lower,
                upper,
            } => {
                println!("Market data: {instrument}, last price: {price}");
                if instrument == config.instrument {
                    quote = Some((lower, upper));
                }
            }
            Event::Order(order) => {
                let reference = text(&order.OrderRef);
                println!(
                    "Order: ref={reference}, status={}, submit_status={}, traded={}",
                    order.OrderStatus as u8 as char,
                    order.OrderSubmitStatus as u8 as char,
                    order.VolumeTraded
                );
                if submitted_reference.as_deref() == Some(reference.as_str()) {
                    if order.OrderSubmitStatus as u8 == sys::THOST_FTDC_OSS_InsertRejected {
                        // OnRtnOrder rejection has StatusMsg, not a separate numeric ErrorID.
                        eprintln!(
                            "Order rejected; StatusMsg: {}",
                            order.status_msg()
                        );
                    }
                    if !cancel_sent
                        && matches!(
                            order.OrderStatus as u8,
                            sys::THOST_FTDC_OST_NoTradeQueueing
                                | sys::THOST_FTDC_OST_PartTradedQueueing
                        )
                    {
                        let request_id = request_ids.next()?;
                        let mut request = InputOrderAction::builder()
                            .broker_id(&config.broker)
                            .investor_id(&config.user)
                            .user_id(&config.user)
                            .instrument_id(&config.instrument)
                            .exchange_id(&text(&order.ExchangeID))
                            .order_ref(&reference)
                            .order_sys_id(&text(&order.OrderSysID))
                            .front_id(order.FrontID)
                            .session_id(order.SessionID)
                            .action_flag(sys::THOST_FTDC_AF_Delete)
                            .order_action_ref(1)
                            .request_id(request_id)
                            .build()?;
                        cancel_sent = true;
                        trader.req_order_action(&mut request, request_id)?;
                        println!(
                            "Cancel submitted for {reference}, request ID: {request_id}; wait for the cancelled order report"
                        );
                    }
                    if order.OrderStatus as u8 == sys::THOST_FTDC_OST_Canceled {
                        println!("Cancellation confirmed for {reference}");
                    }
                }
            }
            Event::Trade(trade) => {
                // A successful request or an order status is not a substitute for this fill report.
                println!(
                    "Trade: ref={}, id={}, price={}, volume={}",
                    text(&trade.OrderRef),
                    text(&trade.TradeID),
                    trade.Price,
                    trade.Volume
                );
            }
            Event::BusinessError { callback, code } => {
                // BusinessErrorCode describes async ErrorID. Immediate request failures instead
                // return RequestError through the API's Result and the `?` operator above.
                return Err(format!("{callback}: {} ({code})", business_message(code)).into());
            }
            Event::MissingResponse(callback) => {
                return Err(format!("{callback}: missing response data").into());
            }
            Event::Disconnected { api, reason } => {
                eprintln!("{api} disconnected: {reason}; waiting for SDK reconnection");
                if api == "Trader" {
                    trader_ready = false;
                } else {
                    md_ready = false;
                    quote = None;
                }
                // Never automatically resubmit an order after reconnecting.
            }
        }

        if let (Some(order), Some((lower, upper))) = (&config.order, quote) {
            if trader_ready && md_ready && submitted_reference.is_none() {
                if !lower.is_finite()
                    || !upper.is_finite()
                    || lower <= 0.0
                    || upper < lower
                    || order.price < lower
                    || order.price > upper
                {
                    return Err("order price is outside the received daily price limits".into());
                }
                let request_id = request_ids.next()?;
                let mut request = InputOrder::builder()
                    .broker_id(&config.broker)
                    .investor_id(&config.user)
                    .user_id(&config.user)
                    .instrument_id(&config.instrument)
                    .exchange_id(&order.exchange)
                    .order_ref(&next_reference)
                    .direction(sys::THOST_FTDC_D_Buy)
                    .order_price_type(sys::THOST_FTDC_OPT_LimitPrice)
                    .comb_offset_flag(sys::THOST_FTDC_OF_Open)
                    .limit_price(order.price)
                    .volume_total_original(order.volume)
                    .time_condition(sys::THOST_FTDC_TC_GFD)
                    .volume_condition(sys::THOST_FTDC_VC_AV)
                    .min_volume(1)
                    .contingent_condition(sys::THOST_FTDC_CC_Immediately)
                    .force_close_reason(sys::THOST_FTDC_FCC_NotForceClose)
                    .is_auto_suspend(0)
                    .user_force_close(0)
                    .request_id(request_id)
                    .build()?;
                // Hedge flags default to speculation; the builder fills the SDK arrays internally.
                // Exactly one attempt; no blind retry after a timeout or reconnect.
                submitted_reference = Some(next_reference.clone());
                trader.req_order_insert(&mut request, request_id)?;
                println!(
                    "BUY/OPEN submitted: ref={next_reference}, request ID: {request_id}, price={}, volume={}; this is not a fill confirmation",
                    order.price, order.volume
                );
            }
        }
    }

    println!("Example duration elapsed; releasing APIs and their owned SPI handlers");
    if let Some(reference) = submitted_reference {
        eprintln!(
            "Check final state for order {reference}: shutdown does not cancel working orders or close positions"
        );
    }
    md.release();
    trader.release();
    Ok(())
}
