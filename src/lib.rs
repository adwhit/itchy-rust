//! itchy - a nom-based parser for the NASDAQ ITCH protocol 5.0
//!
//! It aims to sensibly handle the whole protocol.
//! It is zero-allocation and pretty fast. It will process
//! several million messages per second on a decent CPU.
//!
//! Typical usage:
//!
//! ```ignore
//! extern crate itchy;
//!
//! let stream = itchy::MessageStream::from_file("/path/to/file.itch").unwrap();
//! for msg in stream {
//!     println!("{:?}", msg.unwrap())
//! }
//! ```
//!
//! The protocol specification can be found on the [NASDAQ website](http://www.nasdaqtrader.com/content/technicalsupport/specifications/dataproducts/NQTVITCHSpecification_5.0.pdf)

#[macro_use]
extern crate error_chain;
#[macro_use]
extern crate nom;
extern crate flate2;
extern crate arrayvec;
extern crate decimal;

use std::io::prelude::*;
use std::fs::File;
use std::path::Path;
use std::fmt;
pub use decimal::d128;

use flate2::read::GzDecoder;
use nom::{be_u8, be_u16, be_u32, be_u64, IResult, Needed};
pub use arrayvec::ArrayString;

/// Stack-allocated string of size 4 bytes (re-exported from `arrayvec`)
pub type ArrayString4 = ArrayString<[u8; 4]>;

/// Stack-allocated string of size 8 bytes (re-exported from `arrayvec`)
pub type ArrayString8 = ArrayString<[u8; 8]>;

use errors::*;
pub use enums::*;
use enums::parse_issue_subtype;

const BUFSIZE: usize = 8 * 1024;

mod enums;

pub mod errors {
    error_chain!{
        foreign_links {
            Io(::std::io::Error);
            Nom(::nom::Err);
        }
    }
}

/// Represents an iterable stream of ITCH protocol messages
pub struct MessageStream<R> {
    reader: R,
    buffer: Box<[u8; BUFSIZE]>,
    bufstart: usize,
    bufend: usize,
    bytes_read: usize,
    read_calls: u32,
    message_ct: u32,
    in_error_state: bool,
}

impl MessageStream<File> {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<MessageStream<File>> {
        let reader = File::open(path)?;
        Ok(MessageStream::from_reader(reader))
    }
}

impl MessageStream<GzDecoder<File>> {
    pub fn from_gzip<P: AsRef<Path>>(path: P) -> Result<MessageStream<GzDecoder<File>>> {
        let file = File::open(path)?;
        let reader = GzDecoder::new(file)?;
        Ok(MessageStream::from_reader(reader))
    }
}

impl<R> fmt::Debug for MessageStream<R> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "MessageStream {{ read_calls: {}, bytes_read: {}, buffer_pos: {}, message_ct: {} }}",
            self.read_calls,
            self.bytes_read,
            self.bytes_read - (self.bufend - self.bufstart),
            self.message_ct
        )
    }
}

impl<R: Read> MessageStream<R> {
    pub fn from_reader(reader: R) -> MessageStream<R> {
        MessageStream::new(reader)
    }

    fn new(reader: R) -> MessageStream<R> {
        MessageStream {
            reader,
            buffer: Box::new([0; BUFSIZE]),
            bufstart: 0,
            bufend: 0,
            bytes_read: 0,
            read_calls: 0,
            message_ct: 0,
            in_error_state: false,
        }
    }

    fn fetch_more_bytes(&mut self) -> Result<usize> {
        self.read_calls += 1;
        if self.bufend == BUFSIZE {
            // we need more data from the reader, but first,
            // copy the remnants back to the beginning of the buffer
            // (this should only be a few bytes)
            assert!(self.bufstart as usize > BUFSIZE / 2); // safety check
            // TODO this appears to assume that the buffer was 'full' to start with
            assert!(BUFSIZE - self.bufstart < 100); // extra careful check
            {
                let (left, right) = self.buffer.split_at_mut(self.bufstart);
                &left[..right.len()].copy_from_slice(&right[..]);
                self.bufstart = 0;
                self.bufend = right.len();
            }

        }
        Ok(self.reader.read(&mut self.buffer[self.bufend..])?)
    }
}

impl<R: Read> Iterator for MessageStream<R> {
    type Item = Result<Message>;

    fn next(&mut self) -> Option<Result<Message>> {
        use IResult::*;
        {
            let buf = &self.buffer[self.bufstart..self.bufend];
            match parse_message(buf) {
                Done(rest, msg) => {
                    // TODO could this logic be sped up? Or is it already pretty fast?
                    // it should just consist of pointer arithmetic
                    self.bufstart = self.bufend - rest.len();
                    self.message_ct += 1;
                    self.in_error_state = false;
                    return Some(Ok(msg));
                }
                Error(e) => {
                    // We need to inform user of error, but don't want to get
                    // stuck in an infinite loop if error is ignored
                    // (but obviously shouldn't fail silently on error either)
                    // therefore track if we already in an 'error state' and bail if so
                    if self.in_error_state {
                        return None;
                    } else {
                        self.in_error_state = true;
                        return Some(Err(format!("Parse failed: {}", e).into()));
                    }
                }
                Incomplete(_) => {
                    // fall through to below... necessary to appease borrow checker
                }
            }
        }
        match self.fetch_more_bytes() {
            Ok(0) => {
                // Are we part-way through a parse? If not, assume we are done
                if self.bufstart == self.bufend {
                    return None;
                }
                if self.in_error_state {
                    return None;
                } else {
                    self.in_error_state = true;
                    Some(Err("Unexpected EOF".into()))
                }
            }
            Ok(ct) => {
                self.bufend += ct;
                self.bytes_read += ct;
                self.next()
            }
            Err(e) => {
                if self.in_error_state {
                    return None;
                } else {
                    self.in_error_state = true;
                    Some(Err(e))
                }
            }
        }
    }
}

/// Opaque type representing a price to four decimal places
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Price4(u32);

impl Into<d128> for Price4 {
    fn into(self) -> d128 {
        d128::from(self.0) / d128::from(10_000)
    }
}

impl From<u32> for Price4 {
    fn from(v: u32) -> Price4 {
        Price4(v)
    }
}

/// Opaque type representing a price to eight decimal places
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Price8(u64);

impl Into<d128> for Price8 {
    fn into(self) -> d128 {
        d128::from(self.0) / d128::from(100_000_000)
    }
}

impl From<u64> for Price8 {
    fn from(v: u64) -> Price8 {
        Price8(v)
    }
}
named!(char2bool<bool>, alt!(
    char!('Y') => {|_| true} |
    char!('N') => {|_| false}
));

named!(maybe_char2bool<Option<bool>>, alt!(
    char!('Y') => {|_| Some(true)} |
    char!('N') => {|_| Some(false)} |
    char!(' ') => {|_| None}
));

named!(stock<ArrayString8>, map!(take_str!(8), |s| ArrayString::from(s).unwrap()));

#[inline]
fn be_u48(i: &[u8]) -> IResult<&[u8], u64> {
    if i.len() < 6 {
        IResult::Incomplete(Needed::Size(6))
    } else {
        let res = ((i[0] as u64) << 40) + ((i[1] as u64) << 32) + ((i[2] as u64) << 24) +
            ((i[3] as u64) << 16) + ((i[4] as u64) << 8) + i[5] as u64;
        IResult::Done(&i[6..], res)
    }
}

/// An ITCH protocol message. Refer to the protocol spec for interpretation.
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    pub tag: u8,
    pub stock_locate: u16,
    pub tracking_number: u16,
    pub timestamp: u64,
    pub body: Body,
}

/// The message body. Refer to the protocol spec for interpretation.
#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    AddOrder(AddOrder),
    Breach(LevelBreached),
    BrokenTrade { match_number: u64 },
    CrossTrade(CrossTrade),
    DeleteOrder { reference: u64 },
    Imbalance(ImbalanceIndicator),
    IpoQuotingPeriod(IpoQuotingPeriod),
    MwcbDeclineLevel {
        level1: Price8,
        level2: Price8,
        level3: Price8,
    },
    NonCrossTrade(NonCrossTrade),
    OrderCancelled { reference: u64, cancelled: u32 },
    OrderExecuted {
        reference: u64,
        executed: u32,
        match_number: u64,
    },
    OrderExecutedWithPrice {
        reference: u64,
        executed: u32,
        match_number: u64,
        printable: bool,
        price: Price4,
    },
    ParticipantPosition(MarketParticipantPosition),
    RegShoRestriction {
        stock: ArrayString8,
        action: RegShoAction,
    },
    ReplaceOrder(ReplaceOrder),
    StockDirectory(StockDirectory),
    SystemEvent { event: EventCode },
    TradingAction {
        stock: ArrayString8,
        trading_state: TradingState,
        reason: ArrayString4,
    },
}

named!(parse_message<Message>, do_parse!(
    length: be_u16 >>
    tag: be_u8 >>
    stock_locate: be_u16 >>
    tracking_number: be_u16 >>
    timestamp: be_u48 >>
    body: switch!(value!(tag),
        b'A' => map!(apply!(parse_add_order, false), |order| Body::AddOrder(order)) |
        b'B' => map!(be_u64, |match_number| Body::BrokenTrade{ match_number }) |
        b'C' => do_parse!(reference: be_u64 >> executed: be_u32 >> match_number: be_u64 >>
                          printable: char2bool >> price: be_u32 >>
                          (Body::OrderExecutedWithPrice{
                              reference, executed, match_number,
                              printable, price: price.into() })) |
        b'D' => map!(be_u64, |reference| Body::DeleteOrder{ reference }) |
        b'E' => do_parse!(reference: be_u64 >> executed: be_u32 >> match_number: be_u64 >>
                          (Body::OrderExecuted{ reference, executed, match_number })) |
        b'F' => map!(apply!(parse_add_order, true), |order| Body::AddOrder(order)) |
        b'H' => call!(parse_trading_action) |
        b'I' => map!(parse_imbalance_indicator, |pii| Body::Imbalance(pii)) |
        b'K' => map!(parse_ipo_quoting_period, |ip| Body::IpoQuotingPeriod(ip)) |
        b'L' => map!(parse_participant_position, |pp| Body::ParticipantPosition(pp)) |
        b'P' => map!(parse_noncross_trade, |nt| Body::NonCrossTrade(nt)) |
        b'Q' => map!(parse_cross_trade, |ct| Body::CrossTrade(ct)) |
        b'R' => map!(parse_stock_directory, |sd| Body::StockDirectory(sd)) |
        b'S' => call!(parse_system_event) |
        b'U' => map!(parse_replace_order, |order| Body::ReplaceOrder(order)) |
        b'V' => do_parse!(l1: be_u64 >> l2: be_u64 >> l3: be_u64 >>
                          (Body::MwcbDeclineLevel { level1: l1.into(),
                                                           level2: l2.into(),
                                                           level3: l3.into() })) |
        b'W' => map!(alt!(
            char!('1') => {|_| LevelBreached::L1 } |
            char!('2') => {|_| LevelBreached::L2 } |
            char!('3') => {|_| LevelBreached::L3 }
        ), |l| Body::Breach(l)) |
        b'X' => do_parse!(reference: be_u64 >> cancelled: be_u32 >>
                          (Body::OrderCancelled { reference, cancelled })) |
        b'Y' => call!(parse_reg_sho_restriction)) >>
    (Message { tag, stock_locate, tracking_number, timestamp, body })
));


#[derive(Debug, Clone, PartialEq)]
pub struct StockDirectory {
    pub stock: ArrayString8,
    pub market_category: MarketCategory,
    pub financial_status: FinancialStatus,
    pub round_lot_size: u32,
    pub round_lots_only: bool,
    pub issue_classification: IssueClassification,
    pub issue_subtype: IssueSubType,
    pub authenticity: bool,
    pub short_sale_threshold: Option<bool>,
    pub ipo_flag: Option<bool>,
    pub luld_ref_price_tier: LuldRefPriceTier,
    pub etp_flag: Option<bool>,
    pub etp_leverage_factor: u32,
    pub inverse_indicator: bool,
}

named!(parse_system_event<Body>, do_parse!(
    event_code: alt!(
        char!('O') => { |_| EventCode::StartOfMessages } |
        char!('S') => { |_| EventCode::StartOfSystemHours } |
        char!('Q') => { |_| EventCode::StartOfMarketHours } |
        char!('M') => { |_| EventCode::EndOfMarketHours } |
        char!('E') => { |_| EventCode::EndOfSystemHours } |
        char!('C') => { |_| EventCode::EndOfMessages }
    ) >>
    (Body::SystemEvent{event: event_code})
));

named!(parse_stock_directory<StockDirectory>, do_parse!(
    stock: stock >>
    market_category: alt!(
        char!('Q') => { |_| MarketCategory::NasdaqGlobalSelect } |
        char!('G') => { |_| MarketCategory::NasdaqGlobalMarket } |
        char!('S') => { |_| MarketCategory::NasdaqCaptialMarket } |
        char!('N') => { |_| MarketCategory::Nyse } |
        char!('A') => { |_| MarketCategory::NyseMkt } |
        char!('P') => { |_| MarketCategory::NyseArca } |
        char!('Z') => { |_| MarketCategory::BatsZExchange } |
        char!('V') => { |_| MarketCategory::InvestorsExchange } |
        char!(' ') => { |_| MarketCategory::Unavailable }
    ) >>
    financial_status: alt!(
        char!('N') => { |_| FinancialStatus::Normal } |
        char!('D') => { |_| FinancialStatus::Deficient } |
        char!('E') => { |_| FinancialStatus::Delinquent } |
        char!('Q') => { |_| FinancialStatus::Bankrupt } |
        char!('S') => { |_| FinancialStatus::Suspended } |
        char!('G') => { |_| FinancialStatus::DeficientBankrupt } |
        char!('H') => { |_| FinancialStatus::DeficientDelinquent } |
        char!('J') => { |_| FinancialStatus::DelinquentBankrupt } |
        char!('K') => { |_| FinancialStatus::DeficientDelinquentBankrupt } |
        char!('C') => { |_| FinancialStatus::EtpSuspended } |
        char!(' ') => { |_| FinancialStatus::Unavailable }
    ) >>
    round_lot_size: be_u32 >>
    round_lots_only: char2bool >>
    issue_classification: parse_issue_classification >>
    issue_subtype: parse_issue_subtype >>
    authenticity: alt!(
        char!('P') => {|_| true} |
        char!('T') => {|_| false}
    ) >>
    short_sale_threshold: maybe_char2bool >>
    ipo_flag: maybe_char2bool >>
    luld_ref_price_tier: alt!(
        char!(' ') => { |_| LuldRefPriceTier::Na } |
        char!('1') => { |_| LuldRefPriceTier::Tier1 } |
        char!('2') => { |_| LuldRefPriceTier::Tier2 }
    ) >>
    etp_flag: maybe_char2bool >>
    etp_leverage_factor: be_u32 >>
    inverse_indicator: char2bool >>
    (StockDirectory {
        stock, market_category, financial_status, round_lot_size,
        round_lots_only, issue_classification, issue_subtype,
        authenticity, short_sale_threshold, ipo_flag,
        luld_ref_price_tier, etp_flag, etp_leverage_factor, inverse_indicator
    })
));

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketParticipantPosition {
    pub mpid: ArrayString4,
    pub stock: ArrayString8,
    pub primary_market_maker: bool,
    pub market_maker_mode: MarketMakerMode,
    pub market_participant_state: MarketParticipantState,
}

named!(parse_participant_position<MarketParticipantPosition>, do_parse!(
    mpid: map!(take_str!(4), |s| ArrayString::from(s).unwrap()) >>
    stock: stock >>
    primary_market_maker: char2bool >>
    market_maker_mode: alt!(
        char!('N') => {|_| MarketMakerMode::Normal} |
        char!('P') => {|_| MarketMakerMode::Passive} |
        char!('S') => {|_| MarketMakerMode::Syndicate} |
        char!('R') => {|_| MarketMakerMode::Presyndicate} |
        char!('L') => {|_| MarketMakerMode::Penalty}
    ) >>
    market_participant_state: alt!(
        char!('A') => {|_| MarketParticipantState::Active} |
        char!('E') => {|_| MarketParticipantState::Excused} |
        char!('W') => {|_| MarketParticipantState::Withdrawn} |
        char!('S') => {|_| MarketParticipantState::Suspended} |
        char!('D') => {|_| MarketParticipantState::Deleted}
    ) >>
    (MarketParticipantPosition{
            mpid,
            stock,
            primary_market_maker,
            market_maker_mode,
            market_participant_state
    })
));

named!(parse_reg_sho_restriction<Body>, do_parse!(
    stock: stock >>
    action: alt!(
        char!('0') => {|_| RegShoAction::None} |
        char!('1') => {|_| RegShoAction::Intraday} |
        char!('2') => {|_| RegShoAction::Extant}
    ) >>
    (Body::RegShoRestriction { stock, action })
));

named!(parse_trading_action<Body>, do_parse!(
    stock: stock >>
    trading_state: alt!(
        char!('H') => {|_| TradingState::Halted} |
        char!('P') => {|_| TradingState::Paused} |
        char!('Q') => {|_| TradingState::QuotationOnly} |
        char!('T') => {|_| TradingState::Trading}
    ) >> be_u8 >> // skip reserved byte
    reason: map!(take_str!(4), |s| ArrayString::from(s).unwrap()) >>
    (Body::TradingAction { stock, trading_state, reason })
));


#[derive(Debug, Clone, PartialEq)]
pub struct AddOrder {
    pub reference: u64,
    pub side: Side,
    pub shares: u32,
    pub stock: ArrayString8,
    pub price: Price4,
    pub mpid: Option<ArrayString4>,
}

fn parse_add_order(input: &[u8], attribution: bool) -> IResult<&[u8], AddOrder> {
    do_parse!(input,
    reference: be_u64 >>
    side: alt!(
        char!('B') => {|_| Side::Buy} |
        char!('S') => {|_| Side::Sell}
    ) >>
    shares: be_u32 >>
    stock: stock >>
    price: be_u32 >>
    mpid: cond!(attribution, map!(take_str!(4), |s| ArrayString::from(s).unwrap())) >>
    (AddOrder { reference, side, shares, stock, price: price.into(), mpid })
)
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReplaceOrder {
    pub old_reference: u64,
    pub new_reference: u64,
    pub shares: u32,
    pub price: Price4,
}

named!(parse_replace_order<ReplaceOrder>, do_parse!(
    old_reference: be_u64 >>
    new_reference: be_u64 >>
    shares: be_u32 >>
    price: be_u32 >>
    (ReplaceOrder { old_reference, new_reference, shares, price: price.into() })
));

#[derive(Debug, Clone, PartialEq)]
pub struct ImbalanceIndicator {
    pub paired_shares: u64,
    pub imbalance_shares: u64,
    pub imbalance_direction: ImbalanceDirection,
    pub stock: ArrayString8,
    pub far_price: Price4,
    pub near_price: Price4,
    pub current_ref_price: Price4,
    pub cross_type: CrossType,
    pub price_variation_indicator: char, // TODO encode as enum somehow
}

named!(parse_imbalance_indicator<ImbalanceIndicator>, do_parse!(
    paired_shares: be_u64 >>
        imbalance_shares: be_u64 >>
        imbalance_direction: alt!(
            char!('B') => {|_| ImbalanceDirection::Buy } |
            char!('S') => {|_| ImbalanceDirection::Sell } |
            char!('N') => {|_| ImbalanceDirection::NoImbalance } |
            char!('O') => {|_| ImbalanceDirection::InsufficientOrders }
        ) >>
        stock: stock >>
        far_price: be_u32 >>
        near_price: be_u32 >>
        current_ref_price: be_u32 >>
        cross_type: alt!(
            char!('O') => {|_| CrossType::Opening} |
            char!('C') => {|_| CrossType::Closing} |
            char!('H') => {|_| CrossType::IpoOrHalted}
        ) >>
        price_variation_indicator: be_u8 >>
        (ImbalanceIndicator{paired_shares,
                            imbalance_shares,
                            imbalance_direction,
                            stock,
                            far_price: far_price.into(),
                            near_price: near_price.into(),
                            current_ref_price: current_ref_price.into(),
                            cross_type,
                            price_variation_indicator: price_variation_indicator as char})
));

#[derive(Debug, Clone, PartialEq)]
pub struct CrossTrade {
    pub shares: u64,
    pub stock: ArrayString8,
    pub cross_price: Price4,
    pub match_number: u64,
    pub cross_type: CrossType,
}

named!(parse_cross_trade<CrossTrade>, do_parse!(
    shares: be_u64 >>
    stock: stock >>
    price: be_u32 >>
    match_number: be_u64 >>
    cross_type: alt!(
        char!('O') => {|_| CrossType::Opening} |
        char!('C') => {|_| CrossType::Closing} |
        char!('H') => {|_| CrossType::IpoOrHalted} |
        char!('I') => {|_| CrossType::Intraday}
    ) >>
    (CrossTrade { shares, stock, cross_price: price.into(), match_number, cross_type})
));

#[derive(Debug, Clone, PartialEq)]
pub struct NonCrossTrade {
    pub reference: u64,
    pub side: Side,
    pub shares: u32,
    pub stock: ArrayString8,
    pub price: Price4,
    pub match_number: u64,
}

named!(parse_noncross_trade<NonCrossTrade>, do_parse!(
    reference: be_u64 >>
    side: alt!(
        char!('B') => {|_| Side::Buy} |
        char!('S') => {|_| Side::Sell}
    ) >>
    shares: be_u32 >>
    stock: stock >>
    price: be_u32 >>
    match_number: be_u64 >>
    (NonCrossTrade { reference, side, shares, stock, price: price.into(), match_number })
));

#[derive(Debug, Clone, PartialEq)]
pub struct IpoQuotingPeriod {
    pub stock: ArrayString8,
    pub release_time: u32,
    pub release_qualifier: IpoReleaseQualifier,
    pub price: Price4,
}

named!(parse_ipo_quoting_period<IpoQuotingPeriod>, do_parse!(
    stock: stock >>
        release_time: be_u32 >>
        release_qualifier: alt!(
            char!('A') => { |_| IpoReleaseQualifier::Anticipated } |
            char!('C') => { |_| IpoReleaseQualifier::Cancelled }
        ) >>
        price: be_u32 >>
        (IpoQuotingPeriod { stock, release_time, release_qualifier, price: price.into() })
));
#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn hex_to_bytes(bytes: &[u8]) -> Vec<u8> {
        fn h2b(h: u8) -> Option<u8> {
            match h {
                v @ b'0'...b'9' => Some(v - b'0'),
                v @ b'a'...b'f' => Some(v - b'a' + 10),
                b' ' | b'\n' => None,
                _ => panic!("Invalid hex: {}", h as char),
            }
        }
        bytes
            .iter()
            .filter_map(|b| h2b(*b))
            .collect::<Vec<_>>()
            .chunks(2)
            .map(|slice| (slice[0] << 4) + slice[1])
            .collect()
    }

    #[test]
    fn system_event() {
        let code = b"4f";
        let bytes = hex_to_bytes(&code[..]);
        let (rest, _) = parse_system_event(&bytes[..]).unwrap();
        assert_eq!(rest.len(), 0);
    }

    #[test]
    fn stock_directory() {
        let code = b"41 2020 2020 2020 204e 2000
                     0000 644e 435a 2050 4e20 314e 0000 0000 4e";
        let bytes = hex_to_bytes(&code[..]);
        let (rest, _) = parse_stock_directory(&bytes[..]).unwrap();
        assert_eq!(rest.len(), 0);
    }

    #[test]
    fn market_participant_position() {
        let code = b"41 44 41 4d 42 42 52 59 20 20 20 20 59 4e 41";
        let bytes = hex_to_bytes(&code[..]);
        let (rest, _) = parse_participant_position(&bytes[..]).unwrap();
        assert_eq!(rest.len(), 0);
    }

    #[test]
    fn add_order() {
        let code = b"00 00 00 00 00 00 05 84 42 00 00 00 64 5a 58 5a 5a 54 20 20 20 00 00 27 10";
        let bytes = hex_to_bytes(&code[..]);
        let (rest, _) = parse_add_order(&bytes[..], false).unwrap();
        assert_eq!(rest.len(), 0);
    }

    #[test]
    fn check_sizeof() {
        assert_eq!(std::mem::size_of::<Message>(), 72)
    }

    #[test]
    fn test_imbalance() {
        let code = b"00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 4f 48 49 42 42 20 20 20 20
                     00 00 00 00 00 00 00 00 00 00 00 00 43 20";
        let bytes = hex_to_bytes(&code[..]);
        let (rest, _) = parse_imbalance_indicator(&bytes[..]).unwrap();
        assert_eq!(rest.len(), 0);
    }

    #[test]
    fn test_cross_trade() {
        let code = b"00 00 00 00 00 00 00 00 45 53 53 41 20 20 20 20 00 00
                    00 00 00 00 00 00 00 00 03 c0 43";
        let bytes = hex_to_bytes(&code[..]);
        let (rest, _) = parse_cross_trade(&bytes[..]).unwrap();
        assert_eq!(rest.len(), 0);
    }

    #[test]
    fn test_noncross_trade() {
        let code = b"00 00 00 00 00 00 00 00 42 00 00 0b b8 4e 55 47 54 20
                     20 20 20 00 01 93 e8 00 00 00 00 00 00 41 7f";
        let bytes = hex_to_bytes(&code[..]);
        let (rest, _) = parse_noncross_trade(&bytes[..]).unwrap();
        assert_eq!(rest.len(), 0);
    }

    #[test]
    fn test_ipo_release() {
        let code = b"5a 57 5a 5a 54 20 20 20 00 00 89 1c 41 00 01 86 a0";
        let bytes = hex_to_bytes(&code[..]);
        let (rest, _) = parse_ipo_quoting_period(&bytes[..]).unwrap();
        assert_eq!(rest.len(), 0);
    }

    #[test]
    fn test_parse_empty_buffer() {
        let buf: &[u8] = &[];
        let mut stream = MessageStream::from_reader(buf);
        assert!(stream.next().is_none()); // stops iterating immediately
    }

    #[test]
    fn test_parse_invalid_buffer_fails() {
        let buf: &[u8] = &[0, 0xc, 0x53, 0, 0, 0, 0x28, 0x6a];
        let mut stream = MessageStream::from_reader(buf);
        assert!(stream.next().unwrap().is_err()); // first time gives error
        assert!(stream.next().is_none()); // then it stops iterating
    }

    #[test]
    fn test_parse_one_message() {
        let code = b"000c 5300 0000 0028 6aab 3b3a 994f";
        let buf = hex_to_bytes(&code[..]);
        let mut stream = MessageStream::from_reader(&buf[..]);
        assert!(stream.next().unwrap().is_ok()); // first time ok
        assert!(stream.next().is_none()); // then it stops iterating
    }

    #[test]
    fn test_price4() {
        let p4: d128 = Price4(12340001).into();
        assert_eq!(p4, d128::from_str("1234.0001").unwrap());
    }

    #[test]
    fn test_price8() {
        let p8: d128 = Price8(123400010002).into();
        assert_eq!(p8, d128::from_str("1234.00010002").unwrap());
    }

    fn handle_msg(ix: usize, msg: Result<Message>) {
        match msg {
            Err(e) => panic!("Mesaage {} failed to parse: {}", ix, e),
            Ok(_) => if ix % 1_000_000 == 0 {
                println!("Processed {}M messages", ix / 1000000)
            }
        }
    }

    #[test]
    #[ignore]
    fn full_parse_13_m() {
        let stream = MessageStream::from_file("data/01302016.NASDAQ_ITCH50").unwrap();
        let mut ct = 0;
        for (ix, msg) in stream.enumerate() {
            ct = ix;
            handle_msg(ix, msg)
        }
        assert_eq!(ct, 13761739)
    }

    #[test]
    #[ignore]
    fn full_parse_283_m() {
        let stream = MessageStream::from_gzip("data/01302017.NASDAQ_ITCH50.gz").unwrap();
        let mut ct = 0;
        for (ix, msg) in stream.enumerate() {
            ct = ix;
            handle_msg(ix, msg)
        }
        assert_eq!(ct, 283238831)
    }
}
