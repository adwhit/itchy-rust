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

use core::str;
use std::fs::File;
use std::io::prelude::*;
use std::path::Path;
use std::{fmt, num::NonZero};

pub use arrayvec::ArrayString;
use flate2::read::GzDecoder;
use nom::branch::alt;
use nom::bytes::streaming::take;
use nom::character::streaming::char;
use nom::combinator::map;
use nom::{
    error::ErrorKind,
    number::streaming::{be_u16, be_u32, be_u64, be_u8},
    Err, IResult, Needed,
};

/// Stack-allocated string of size 4 bytes (re-exported from `arrayvec`)
pub type ArrayString4 = ArrayString<4>;

/// Stack-allocated string of size 8 bytes (re-exported from `arrayvec`)
pub type ArrayString8 = ArrayString<8>;

use enums::parse_issue_subtype;
pub use enums::*;
use rust_decimal::Decimal;

mod enums;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("Parse error: {0}")]
    Parse(String),
    #[error(transparent)]
    Io(#[from] ::std::io::Error),
    #[error(transparent)]
    Nom(#[from] ::nom::Err<u32>),
}

type Result<T> = std::result::Result<T, Error>;

// Size of buffer for parsing
const BUFSIZE: usize = 8 * 1024;

/// Represents an iterable stream of ITCH protocol messages
pub struct MessageStream<R> {
    reader: R,
    buffer: Box<[u8; BUFSIZE]>,
    bufstart: usize,
    bufend: usize,
    bytes_read: usize,
    read_calls: u32,
    message_ct: u32, // messages read so far
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
        let reader = GzDecoder::new(file);
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
            assert!(self.bufstart > BUFSIZE / 2); // safety check
                                                  // TODO this appears to assume that the buffer was 'full' to start with
            assert!(BUFSIZE - self.bufstart < 100); // extra careful check
            {
                let (left, right) = self.buffer.split_at_mut(self.bufstart);
                left[..right.len()].copy_from_slice(right);
                self.bufstart = 0;
                self.bufend = right.len();
            }
        }
        Ok(self.reader.read(&mut self.buffer[self.bufend..])?)
    }

    pub fn bytes_read(&self) -> usize {
        self.bytes_read
    }
}

impl<R: Read> Iterator for MessageStream<R> {
    type Item = Result<Message>;

    fn next(&mut self) -> Option<Result<Message>> {
        {
            let buf = &self.buffer[self.bufstart..self.bufend];
            match parse_message(buf) {
                Ok((rest, msg)) => {
                    // TODO could this logic be sped up? Or is it already pretty fast?
                    // it should just consist of pointer arithmetic
                    self.bufstart = self.bufend - rest.len();
                    self.message_ct += 1;
                    self.in_error_state = false;
                    return Some(Ok(msg));
                }
                Err(Err::Error(e)) | Err(Err::Failure(e)) => {
                    // We need to inform user of error, but don't want to get
                    // stuck in an infinite loop if error is ignored
                    // (but obviously shouldn't fail silently on error either)
                    // therefore track if we already in an 'error state' and bail if so
                    if self.in_error_state {
                        return None;
                    } else if e.code != ErrorKind::Eof {
                        self.in_error_state = true;
                        return Some(Err(Error::Parse(format!(
                            "{:?}, buffer context {:?}",
                            e.code,
                            &self.buffer[self.bufstart..self.bufstart + 20]
                        ))));
                    }
                }
                Err(Err::Incomplete(_)) => {
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
                    None
                } else {
                    self.in_error_state = true;
                    Some(Err(Error::Parse("Unexpected EOF".into())))
                }
            }
            Ok(ct) => {
                self.bufend += ct;
                self.bytes_read += ct;
                self.next()
            }
            Err(e) => {
                if self.in_error_state {
                    None
                } else {
                    self.in_error_state = true;
                    Some(Err(e))
                }
            }
        }
    }
}

/// Trait for readers that can report their size.
pub trait KnownSizeReader {
    /// Returns the size of the reader in bytes.
    ///
    /// Note: This function is not guaranteed to be fast. Cache the result if you need to call it multiple times.
    fn size(&self) -> Result<u64>;
}

impl KnownSizeReader for File {
    fn size(&self) -> Result<u64> {
        Ok(self.metadata()?.len())
    }
}

impl KnownSizeReader for GzDecoder<File> {
    fn size(&self) -> Result<u64> {
        Ok(self.get_ref().metadata()?.len())
    }
}

impl<R> MessageStream<R>
where
    R: KnownSizeReader,
{
    /// Returns the size of the underlying reader in bytes.
    pub fn reader_size(&self) -> Result<u64> {
        self.reader.size()
    }
}

/// Opaque type representing a price to four decimal places
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Price4(u32);

impl Price4 {
    pub fn raw(self) -> u32 {
        self.0
    }
}

impl From<Price4> for Decimal {
    fn from(val: Price4) -> Self {
        Self::from(val.0) / Self::from(10_000)
    }
}

impl From<u32> for Price4 {
    fn from(v: u32) -> Price4 {
        Price4(v)
    }
}

/// Opaque type representing a price to eight decimal places
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct Price8(u64);

impl Price8 {
    pub fn raw(self) -> u64 {
        self.0
    }
}

impl From<Price8> for Decimal {
    fn from(val: Price8) -> Self {
        Decimal::from(val.0) / Decimal::from(100_000_000)
    }
}

impl From<u64> for Price8 {
    fn from(v: u64) -> Price8 {
        Price8(v)
    }
}

fn char2bool(input: &[u8]) -> IResult<&[u8], bool> {
    alt((map(char('Y'), |_| true), map(char('N'), |_| false)))(input)
}

fn maybe_char2bool(input: &[u8]) -> IResult<&[u8], Option<bool>> {
    alt((
        map(char('Y'), |_| Some(true)),
        map(char('N'), |_| Some(false)),
        map(char(' '), |_| None),
    ))(input)
}

fn parse_etp_flag(input: &[u8]) -> IResult<&[u8], Option<bool>> {
    alt((
        map(char('Y'), |_| Some(true)),
        map(char('N'), |_| Some(false)),
        map(char(' '), |_| None),
        map(char('M'), |_| Some(true)),
    ))(input)
}

fn stock(input: &[u8]) -> IResult<&[u8], ArrayString8> {
    map(take(8usize), |s: &[u8]| {
        ArrayString::from(str::from_utf8(s).unwrap()).unwrap()
    })(input)
}

#[inline]
fn be_u48(i: &[u8]) -> IResult<&[u8], u64> {
    if i.len() < 6 {
        IResult::Err(Err::Incomplete(Needed::Size(unsafe {
            NonZero::new_unchecked(6)
        })))
    } else {
        let res = ((i[0] as u64) << 40)
            + ((i[1] as u64) << 32)
            + ((i[2] as u64) << 24)
            + ((i[3] as u64) << 16)
            + ((i[4] as u64) << 8)
            + i[5] as u64;
        IResult::Ok((&i[6..], res))
    }
}

/// An ITCH protocol message. Refer to the protocol spec for interpretation.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Message {
    /// Message Type
    pub tag: u8,
    /// Integer identifying the underlying instrument updated daily
    pub stock_locate: u16,
    /// NASDAQ internal tracking number
    pub tracking_number: u16,
    /// Nanoseconds since midnight
    pub timestamp: u64,
    /// Body of one of the supported message types
    pub body: Body,
}

/// The message body. Refer to the protocol spec for interpretation.
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub enum Body {
    AddOrder(AddOrder),
    Breach(LevelBreached),
    BrokenTrade {
        match_number: u64,
    },
    CrossTrade(CrossTrade),
    DeleteOrder {
        reference: u64,
    },
    Imbalance(ImbalanceIndicator),
    IpoQuotingPeriod(IpoQuotingPeriod),
    LULDAuctionCollar {
        stock: ArrayString8,
        ref_price: Price4,
        upper_price: Price4,
        lower_price: Price4,
        extension: u32,
    },
    MwcbDeclineLevel {
        level1: Price8,
        level2: Price8,
        level3: Price8,
    },
    NonCrossTrade(NonCrossTrade),
    OrderCancelled {
        reference: u64,
        cancelled: u32,
    },
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
    SystemEvent {
        event: EventCode,
    },
    TradingAction {
        stock: ArrayString8,
        trading_state: TradingState,
        reason: ArrayString4,
    },
    RetailPriceImprovementIndicator(RetailPriceImprovementIndicator),
}

fn parse_message(input: &[u8]) -> IResult<&[u8], Message> {
    let (input, _length) = be_u16(input)?;
    let (input, tag) = be_u8(input)?;
    let (input, stock_locate) = be_u16(input)?;
    let (input, tracking_number) = be_u16(input)?;
    let (input, timestamp) = be_u48(input)?;
    let (input, body) = match tag {
        b'A' => {
            let (input, add_order) = parse_add_order(input, false)?;
            (input, Body::AddOrder(add_order))
        }
        b'B' => map(be_u64, |match_number| Body::BrokenTrade { match_number })(input)?,
        b'C' => {
            let (input, reference) = be_u64(input)?;
            let (input, executed) = be_u32(input)?;
            let (input, match_number) = be_u64(input)?;
            let (input, printable) = char2bool(input)?;
            let (input, price) = be_u32(input)?;
            (
                input,
                Body::OrderExecutedWithPrice {
                    reference,
                    executed,
                    match_number,
                    printable,
                    price: price.into(),
                },
            )
        }
        b'D' => map(be_u64, |reference| Body::DeleteOrder { reference })(input)?,
        b'E' => {
            let (input, reference) = be_u64(input)?;
            let (input, executed) = be_u32(input)?;
            let (input, match_number) = be_u64(input)?;
            (
                input,
                Body::OrderExecuted {
                    reference,
                    executed,
                    match_number,
                },
            )
        }
        b'F' => {
            let (input, add_order) = parse_add_order(input, true)?;
            (input, Body::AddOrder(add_order))
        }
        b'H' => parse_trading_action(input)?,
        b'I' => map(parse_imbalance_indicator, Body::Imbalance)(input)?,
        b'J' => {
            let (input, stock) = stock(input)?;
            let (input, ref_p) = be_u32(input)?;
            let (input, upper_p) = be_u32(input)?;
            let (input, lower_p) = be_u32(input)?;
            let (input, extension) = be_u32(input)?;
            (
                input,
                Body::LULDAuctionCollar {
                    stock,
                    ref_price: ref_p.into(),
                    upper_price: upper_p.into(),
                    lower_price: lower_p.into(),
                    extension,
                },
            )
        }
        b'K' => map(parse_ipo_quoting_period, Body::IpoQuotingPeriod)(input)?,
        b'L' => map(parse_participant_position, Body::ParticipantPosition)(input)?,
        b'N' => map(
            parse_retail_price_improvement_indicator,
            Body::RetailPriceImprovementIndicator,
        )(input)?,
        b'P' => map(parse_noncross_trade, Body::NonCrossTrade)(input)?,
        b'Q' => map(parse_cross_trade, Body::CrossTrade)(input)?,
        b'R' => map(parse_stock_directory, Body::StockDirectory)(input)?,
        b'S' => parse_system_event(input)?,
        b'U' => map(parse_replace_order, Body::ReplaceOrder)(input)?,
        b'V' => {
            let (input, l1) = be_u64(input)?;
            let (input, l2) = be_u64(input)?;
            let (input, l3) = be_u64(input)?;
            (
                input,
                Body::MwcbDeclineLevel {
                    level1: l1.into(),
                    level2: l2.into(),
                    level3: l3.into(),
                },
            )
        }
        b'W' => map(
            alt((
                map(char('1'), |_| LevelBreached::L1),
                map(char('2'), |_| LevelBreached::L2),
                map(char('3'), |_| LevelBreached::L3),
            )),
            Body::Breach,
        )(input)?,
        b'X' => {
            let (input, reference) = be_u64(input)?;
            let (input, cancelled) = be_u32(input)?;
            (
                input,
                Body::OrderCancelled {
                    reference,
                    cancelled,
                },
            )
        }
        b'Y' => parse_reg_sho_restriction(input)?,
        _ => unreachable!(),
    };

    Ok((
        input,
        Message {
            tag,
            stock_locate,
            tracking_number,
            timestamp,
            body,
        },
    ))
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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

fn parse_system_event(input: &[u8]) -> IResult<&[u8], Body> {
    let (input, event_code) = alt((
        map(char('O'), |_| EventCode::StartOfMessages),
        map(char('S'), |_| EventCode::StartOfSystemHours),
        map(char('Q'), |_| EventCode::StartOfMarketHours),
        map(char('M'), |_| EventCode::EndOfMarketHours),
        map(char('E'), |_| EventCode::EndOfSystemHours),
        map(char('C'), |_| EventCode::EndOfMessages),
    ))(input)?;

    Ok((input, Body::SystemEvent { event: event_code }))
}

fn parse_stock_directory(input: &[u8]) -> IResult<&[u8], StockDirectory> {
    let (input, stock) = stock(input)?;
    let (input, market_category) = alt((
        map(char('Q'), |_| MarketCategory::NasdaqGlobalSelect),
        map(char('G'), |_| MarketCategory::NasdaqGlobalMarket),
        map(char('S'), |_| MarketCategory::NasdaqCapitalMarket),
        map(char('N'), |_| MarketCategory::Nyse),
        map(char('A'), |_| MarketCategory::NyseMkt),
        map(char('P'), |_| MarketCategory::NyseArca),
        map(char('Z'), |_| MarketCategory::BatsZExchange),
        map(char('V'), |_| MarketCategory::InvestorsExchange),
        map(char(' '), |_| MarketCategory::Unavailable),
    ))(input)?;
    let (input, financial_status) = alt((
        map(char('N'), |_| FinancialStatus::Normal),
        map(char('D'), |_| FinancialStatus::Deficient),
        map(char('E'), |_| FinancialStatus::Delinquent),
        map(char('Q'), |_| FinancialStatus::Bankrupt),
        map(char('S'), |_| FinancialStatus::Suspended),
        map(char('G'), |_| FinancialStatus::DeficientBankrupt),
        map(char('H'), |_| FinancialStatus::DeficientDelinquent),
        map(char('J'), |_| FinancialStatus::DelinquentBankrupt),
        map(char('K'), |_| FinancialStatus::DeficientDelinquentBankrupt),
        map(char('C'), |_| FinancialStatus::EtpSuspended),
        map(char(' '), |_| FinancialStatus::Unavailable),
    ))(input)?;
    let (input, round_lot_size) = be_u32(input)?;
    let (input, round_lots_only) = char2bool(input)?;
    let (input, issue_classification) = parse_issue_classification(input)?;
    let (input, issue_subtype) = parse_issue_subtype(input)?;
    let (input, authenticity) = alt((map(char('P'), |_| true), map(char('T'), |_| false)))(input)?;
    let (input, short_sale_threshold) = maybe_char2bool(input)?;
    let (input, ipo_flag) = maybe_char2bool(input)?;
    let (input, luld_ref_price_tier) = alt((
        map(char(' '), |_| LuldRefPriceTier::Na),
        map(char('1'), |_| LuldRefPriceTier::Tier1),
        map(char('2'), |_| LuldRefPriceTier::Tier2),
    ))(input)?;
    let (input, etp_flag) = parse_etp_flag(input)?;
    let (input, etp_leverage_factor) = be_u32(input)?;
    let (input, inverse_indicator) = char2bool(input)?;

    Ok((
        input,
        StockDirectory {
            stock,
            market_category,
            financial_status,
            round_lot_size,
            round_lots_only,
            issue_classification,
            issue_subtype,
            authenticity,
            short_sale_threshold,
            ipo_flag,
            luld_ref_price_tier,
            etp_flag,
            etp_leverage_factor,
            inverse_indicator,
        },
    ))
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MarketParticipantPosition {
    pub mpid: ArrayString4,
    pub stock: ArrayString8,
    pub primary_market_maker: bool,
    pub market_maker_mode: MarketMakerMode,
    pub market_participant_state: MarketParticipantState,
}

fn parse_participant_position(input: &[u8]) -> IResult<&[u8], MarketParticipantPosition> {
    let (input, mpid) = map(take(4usize), |s: &[u8]| {
        ArrayString::from(str::from_utf8(s).unwrap()).unwrap()
    })(input)?;
    let (input, stock) = stock(input)?;
    let (input, primary_market_maker) = char2bool(input)?;
    let (input, market_maker_mode) = alt((
        map(char('N'), |_| MarketMakerMode::Normal),
        map(char('P'), |_| MarketMakerMode::Passive),
        map(char('S'), |_| MarketMakerMode::Syndicate),
        map(char('R'), |_| MarketMakerMode::Presyndicate),
        map(char('L'), |_| MarketMakerMode::Penalty),
    ))(input)?;
    let (input, market_participant_state) = alt((
        map(char('A'), |_| MarketParticipantState::Active),
        map(char('E'), |_| MarketParticipantState::Excused),
        map(char('W'), |_| MarketParticipantState::Withdrawn),
        map(char('S'), |_| MarketParticipantState::Suspended),
        map(char('D'), |_| MarketParticipantState::Deleted),
    ))(input)?;

    Ok((
        input,
        MarketParticipantPosition {
            mpid,
            stock,
            primary_market_maker,
            market_maker_mode,
            market_participant_state,
        },
    ))
}

fn parse_reg_sho_restriction(input: &[u8]) -> IResult<&[u8], Body> {
    let (input, stock) = stock(input)?;
    let (input, action) = alt((
        map(char('0'), |_| RegShoAction::None),
        map(char('1'), |_| RegShoAction::Intraday),
        map(char('2'), |_| RegShoAction::Extant),
    ))(input)?;

    Ok((input, Body::RegShoRestriction { stock, action }))
}

fn parse_trading_action(input: &[u8]) -> IResult<&[u8], Body> {
    let (input, stock) = stock(input)?;
    let (input, trading_state) = alt((
        map(char('H'), |_| TradingState::Halted),
        map(char('P'), |_| TradingState::Paused),
        map(char('Q'), |_| TradingState::QuotationOnly),
        map(char('T'), |_| TradingState::Trading),
    ))(input)?;
    let (input, _) = be_u8(input)?; // skip reserved byte
    let (input, reason) = map(take(4usize), |s: &[u8]| {
        ArrayString::from(str::from_utf8(s).unwrap()).unwrap()
    })(input)?;

    Ok((
        input,
        Body::TradingAction {
            stock,
            trading_state,
            reason,
        },
    ))
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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
    let (input, reference) = be_u64(input)?;
    let (input, side) = alt((
        map(char('B'), |_| Side::Buy),
        map(char('S'), |_| Side::Sell),
    ))(input)?;
    let (input, shares) = be_u32(input)?;
    let (input, stock) = stock(input)?;
    let (input, price) = be_u32(input)?;
    let (input, mpid) = match attribution {
        true => map(take(4usize), |s: &[u8]| {
            Some(ArrayString::from(str::from_utf8(s).unwrap()).unwrap())
        })(input),
        false => Ok((input, None)),
    }?;

    Ok((
        input,
        AddOrder {
            reference,
            side,
            shares,
            stock,
            price: price.into(),
            mpid,
        },
    ))
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct ReplaceOrder {
    pub old_reference: u64,
    pub new_reference: u64,
    pub shares: u32,
    pub price: Price4,
}

fn parse_replace_order(input: &[u8]) -> IResult<&[u8], ReplaceOrder> {
    let (input, old_reference) = be_u64(input)?;
    let (input, new_reference) = be_u64(input)?;
    let (input, shares) = be_u32(input)?;
    let (input, price) = be_u32(input)?;

    Ok((
        input,
        ReplaceOrder {
            old_reference,
            new_reference,
            shares,
            price: price.into(),
        },
    ))
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
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

fn parse_imbalance_indicator(input: &[u8]) -> IResult<&[u8], ImbalanceIndicator> {
    let (input, paired_shares) = be_u64(input)?;
    let (input, imbalance_shares) = be_u64(input)?;
    let (input, imbalance_direction) = alt((
        map(char('B'), |_| ImbalanceDirection::Buy),
        map(char('S'), |_| ImbalanceDirection::Sell),
        map(char('N'), |_| ImbalanceDirection::NoImbalance),
        map(char('O'), |_| ImbalanceDirection::InsufficientOrders),
    ))(input)?;
    let (input, stock) = stock(input)?;
    let (input, far_price) = be_u32(input)?;
    let (input, near_price) = be_u32(input)?;
    let (input, current_ref_price) = be_u32(input)?;
    let (input, cross_type) = alt((
        map(char('O'), |_| CrossType::Opening),
        map(char('C'), |_| CrossType::Closing),
        map(char('H'), |_| CrossType::IpoOrHalted),
        map(char('A'), |_| CrossType::ExtendedTradingClose),
    ))(input)?;
    let (input, price_variation_indicator) = be_u8(input)?;

    Ok((
        input,
        ImbalanceIndicator {
            paired_shares,
            imbalance_shares,
            imbalance_direction,
            stock,
            far_price: far_price.into(),
            near_price: near_price.into(),
            current_ref_price: current_ref_price.into(),
            cross_type,
            price_variation_indicator: price_variation_indicator as char,
        },
    ))
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct CrossTrade {
    pub shares: u64,
    pub stock: ArrayString8,
    pub cross_price: Price4,
    pub match_number: u64,
    pub cross_type: CrossType,
}

fn parse_cross_trade(input: &[u8]) -> IResult<&[u8], CrossTrade> {
    let (input, shares) = be_u64(input)?;
    let (input, stock) = stock(input)?;
    let (input, price) = be_u32(input)?;
    let (input, match_number) = be_u64(input)?;
    let (input, cross_type) = alt((
        map(char('O'), |_| CrossType::Opening),
        map(char('C'), |_| CrossType::Closing),
        map(char('H'), |_| CrossType::IpoOrHalted),
        map(char('I'), |_| CrossType::Intraday),
        map(char('A'), |_| CrossType::ExtendedTradingClose),
    ))(input)?;

    Ok((
        input,
        CrossTrade {
            shares,
            stock,
            cross_price: price.into(),
            match_number,
            cross_type,
        },
    ))
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct RetailPriceImprovementIndicator {
    pub stock: ArrayString8,
    pub interest_flag: InterestFlag,
}

fn parse_retail_price_improvement_indicator(
    input: &[u8],
) -> IResult<&[u8], RetailPriceImprovementIndicator> {
    let (input, stock) = stock(input)?;
    let (input, interest_flag) = alt((
        map(char('B'), |_| InterestFlag::RPIAvailableBuySide),
        map(char('S'), |_| InterestFlag::RPIAvailableSellSide),
        map(char('A'), |_| InterestFlag::RPIAvailableBothSides),
        map(char('N'), |_| InterestFlag::RPINoneAvailable),
    ))(input)?;

    Ok((
        input,
        RetailPriceImprovementIndicator {
            stock,
            interest_flag,
        },
    ))
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct NonCrossTrade {
    pub reference: u64,
    pub side: Side,
    pub shares: u32,
    pub stock: ArrayString8,
    pub price: Price4,
    pub match_number: u64,
}

fn parse_noncross_trade(input: &[u8]) -> IResult<&[u8], NonCrossTrade> {
    let (input, reference) = be_u64(input)?;
    let (input, side) = alt((
        map(char('B'), |_| Side::Buy),
        map(char('S'), |_| Side::Sell),
    ))(input)?;
    let (input, shares) = be_u32(input)?;
    let (input, stock) = stock(input)?;
    let (input, price) = be_u32(input)?;
    let (input, match_number) = be_u64(input)?;

    Ok((
        input,
        NonCrossTrade {
            reference,
            side,
            shares,
            stock,
            price: price.into(),
            match_number,
        },
    ))
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub struct IpoQuotingPeriod {
    pub stock: ArrayString8,
    pub release_time: u32,
    pub release_qualifier: IpoReleaseQualifier,
    pub price: Price4,
}

fn parse_ipo_quoting_period(input: &[u8]) -> IResult<&[u8], IpoQuotingPeriod> {
    let (input, stock) = stock(input)?;
    let (input, release_time) = be_u32(input)?;
    let (input, release_qualifier) = alt((
        map(char('A'), |_| IpoReleaseQualifier::Anticipated),
        map(char('C'), |_| IpoReleaseQualifier::Cancelled),
    ))(input)?;
    let (input, price) = be_u32(input)?;

    Ok((
        input,
        IpoQuotingPeriod {
            stock,
            release_time,
            release_qualifier,
            price: price.into(),
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn hex_to_bytes(bytes: &[u8]) -> Vec<u8> {
        fn h2b(h: u8) -> Option<u8> {
            match h {
                v @ b'0'..=b'9' => Some(v - b'0'),
                v @ b'a'..=b'f' => Some(v - b'a' + 10),
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
    fn add_order_with_attr() {
        // same code as in add_order test with 4 additional `10` bytes
        let code = b"00 00 00 00 00 00 05 84 42 00 00 00 64 5a 58 5a 5a 54 20 20 20 00 00 27 10 10 10 10 10";
        let bytes = hex_to_bytes(&code[..]);
        let (rest, _) = parse_add_order(&bytes[..], true).unwrap();
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
    fn test_retail_price_improvement_indicator() {
        let code = b"45 53 53 41 20 20 20 20 4e";
        let bytes = hex_to_bytes(&code[..]);
        let (rest, _) = parse_retail_price_improvement_indicator(&bytes[..]).unwrap();
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
        let p4: Decimal = Price4(12340001).into();
        assert_eq!(p4, Decimal::from_str("1234.0001").unwrap());
    }

    #[test]
    fn test_price8() {
        let p8: Decimal = Price8(123400010002).into();
        assert_eq!(p8, Decimal::from_str("1234.00010002").unwrap());
    }

    fn handle_msg(ix: usize, msg: Result<Message>) {
        match msg {
            Err(e) => panic!("Mesaage {} failed to parse: {}", ix, e),
            Ok(msg) => {
                if ix % 1_000_000 == 0 {
                    println!("Processed {}M messages", ix / 1000000);
                    println!("{:?}", msg)
                }
            }
        }
    }

    #[test]
    #[ignore]
    fn test_full_parse() {
        // Download sample data from ftp://emi.nasdaq.com/ITCH/
        let stream = MessageStream::from_file("sample-data/20190830.PSX_ITCH_50").unwrap();
        let mut ct = 0;
        for (ix, msg) in stream.enumerate() {
            ct = ix;
            handle_msg(ix, msg)
        }
        assert_eq!(ct, 40030397)
    }

    #[cfg(feature = "serde")]
    #[test]
    fn test_serde() {
        let msg = Message {
            tag: 123,
            stock_locate: 234,
            tracking_number: 321,
            timestamp: 3333,
            body: Body::Breach(LevelBreached::L1),
        };
        let blob = serde_json::to_string(&msg).unwrap();
        let msg_2 = serde_json::from_str(&blob).unwrap();
        assert_eq!(msg, msg_2);
    }
}
