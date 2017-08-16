#[macro_use]
extern crate error_chain;
#[macro_use]
extern crate nom;
extern crate flate2;

use std::io::prelude::*;
use std::fs::File;
use std::path::Path;

use flate2::read::GzDecoder;
use nom::{be_u8, be_u16, be_u32, be_u64, print, IResult, Needed};

use errors::*;
pub use enums::*;

const BUFSIZE: usize = 10 * 1024;

mod enums;


#[allow(unused_doc_comment)]
pub mod errors {
    error_chain!{
        foreign_links {
            Io(::std::io::Error);
            Nom(::nom::Err);
        }
    }
}

pub struct MessageStream<R: Read> {
    reader: R,
    buffer: Box<[u8; BUFSIZE]>, // 10Mb buffer
    bufstart: usize,
    bufend: usize,
}

impl<R: Read> MessageStream<R> {
    fn new(reader: R) -> MessageStream<R> {
        MessageStream {
            reader,
            buffer: Box::new([0; BUFSIZE]),
            bufstart: 0,
            bufend: 0,
        }
    }

    fn fetch_more_bytes(&mut self) -> Result<usize> {
        if self.bufend == BUFSIZE {
            // we need more data from the reader
            // first, copy the remnants back to the beginning of the buffer
            // (this should only be a few bytes)
            assert!(self.bufstart as usize > BUFSIZE / 2); // safety check
            assert!(BUFSIZE - self.bufstart < 50); // extra careful check
            {
                let (mut left, right) = self.buffer.split_at_mut(self.bufstart);
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
        match parse_message(&self.buffer[self.bufstart..self.bufend]) {
            Done(rest, msg) => {
                self.bufstart = self.bufend - rest.len();
                return Some(Ok(msg));
            }
            Error(e) => return Some(Err(errors::Error::from(e))),
            Incomplete(_) => {
                // fall through to below... necessary to appease borrow checker
            }
        }
        match self.fetch_more_bytes() {
            Ok(0) => Some(Err("Unexpected EOF".into())),
            Ok(ct) => {
                self.bufend += ct;
                self.next()
            }
            Err(e) => Some(Err(e)),
        }
    }
}

pub fn parse_reader<R: Read>(reader: R) -> MessageStream<R> {
    // We will do the parsing in a streaming fashion because these
    // files are BIG and we don't want to load it all into memory
    MessageStream::new(reader)
}

pub fn parse_gzip<P: AsRef<Path>>(path: P) -> Result<MessageStream<GzDecoder<File>>> {
    let file = File::open(path)?;
    let reader = GzDecoder::new(file)?;
    Ok(parse_reader(reader))
}

pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<MessageStream<File>> {
    let reader = File::open(path)?;
    Ok(parse_reader(reader))
}

#[inline]
pub fn be_u48(i: &[u8]) -> IResult<&[u8], u64> {
    if i.len() < 6 {
        IResult::Incomplete(Needed::Size(6))
    } else {
        let res = ((i[0] as u64) << 40) + ((i[1] as u64) << 32) + ((i[2] as u64) << 24) +
            ((i[3] as u64) << 16) + ((i[4] as u64) << 8) + i[5] as u64;
        IResult::Done(&i[6..], res)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    SystemEvent {
        tracking_number: u16,
        timestamp: u64,
        event_code: EventCode,
    },
    Unknown {
        length: u16,
        tag: char,
        content: Vec<u8>,
    },
    StockDirectory(StockDirectory),
}

#[derive(Debug, Clone, PartialEq)]
pub struct StockDirectory {
    stock_locate: u16,
    tracking_number: u16,
    timestamp: u64,
    stock: String,
    market_category: MarketCategory,
    financial_status: FinancialStatus,
    round_lot_size: u32,
    round_lots_only: bool,
    issue_classification: IssueClassification,
    issue_subtype: IssueSubType,
    authenticity: (),
    short_sale_threshold: Option<bool>,
    ipo_flag: Option<bool>,
    luld_ref_price_tier: LuldRefPriceTier,
    etp_flag: Option<bool>,
    etp_leverage_factor: u32,
    inverse_indicator: bool,
}

named!(parse_message<Message>, do_parse!(
    length: be_u16 >>
    msg: switch!(be_u8,
        b'S' => call!(parse_system_event) |
        b'R' => map!(call!(parse_stock_directory), |sd| Message::StockDirectory(sd)) |
        other => map!(take!(length - 1),
                      |slice| Message::Unknown {
                          length,
                          tag: other as char,
                          content: Vec::from(slice)
                      })) >>
    (msg)
));

named!(parse_system_event<Message>, do_parse!(
    tag!([0, 0]) >>
    tracking_number: be_u16 >>
    timestamp: be_u48 >>
    event_code: alt!(
        char!('O') => { |_| EventCode::StartOfMessages } |
        char!('S') => { |_| EventCode::StartOfSystemHours } |
        char!('Q') => { |_| EventCode::StartOfMarketHours } |
        char!('M') => { |_| EventCode::EndOfMarketHours } |
        char!('E') => { |_| EventCode::EndOfSystemHours } |
        char!('C') => { |_| EventCode::EndOfMessages }
    ) >>
    (Message::SystemEvent { tracking_number, timestamp, event_code })
));

named!(parse_stock_directory<StockDirectory>, do_parse!(
    stock_locate: be_u16 >>
        tracking_number: be_u16 >>
        timestamp: be_u48 >>
        stock: map!(take_str!(8), |s| s.trim().to_string()) >>
        market_category: alt!(
            char!('Q') => { |_| MarketCategory::NasdaqGlobalSelect } |
            char!('G') => { |_| MarketCategory::NasdaqGlobalMarket } |
            char!('S') => { |_| MarketCategory::NasdaqCaptialMarket } |
            char!('N') => { |_| MarketCategory::Nyse } |
            char!('A') => { |_| MarketCategory::NyseMkt } |
            char!('P') => { |_| MarketCategory::NyseArca } |
            char!('Z') => { |_| MarketCategory::BatsZExchange } |
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
        issue_classification: value!(IssueClassification::Unit, take!(1)) >>

        // FIXME these are dummy values
        issue_subtype: value!(IssueSubType::AlphaIndexETNs, take!(2)) >>
        authenticity: value!((), char!('P')) >>

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
            stock_locate, tracking_number, timestamp, stock,
            market_category, financial_status, round_lot_size,
            round_lots_only, issue_classification, issue_subtype,
            authenticity, short_sale_threshold, ipo_flag,
            luld_ref_price_tier, etp_flag, etp_leverage_factor, inverse_indicator
        })
));


named!(char2bool<bool>, alt!(
    char!('Y') => {|_| true} |
    char!('N') => {|_| false}
));

named!(maybe_char2bool<Option<bool>>, alt!(
    char!('Y') => {|_| Some(true)} |
    char!('N') => {|_| Some(false)} |
    char!(' ') => {|_| None}
));

#[cfg(test)]
mod tests {
    use super::*;

    fn hex_to_bytes(bytes: &[u8]) -> Vec<u8> {
        fn h2b(h: u8) -> Option<u8> {
            match h {
                v @ b'0'...b'9' => Some(v - b'0'),
                v @ b'a'...b'f' => Some(v - b'a' + 10),
                b' ' => None,
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
        let code = b"0000 0000 286aab3b3a99 4f";
        let bytes = hex_to_bytes(&code[..]);
        let (_, out) = parse_system_event(&bytes[..]).unwrap();
        if let Message::SystemEvent { .. } = out {
        } else {
            panic!("Expected SystemEvent,  found {:?}", out)
        }
    }

    #[test]
    fn stock_directory() {
        let code = b"00 0100 0028 9d5b 22a4 1b41 2020 2020 2020 204e 2000 0000 644e 435a 2050 4e20 314e 0000 0000 4e";
        let bytes = hex_to_bytes(&code[..]);
        parse_stock_directory(&bytes[..]).unwrap();
    }

    #[test]
    fn full_parse() {
        let mut iter = parse_file("data/01302016.NASDAQ_ITCH50").unwrap();
        for (ix, msg) in iter.enumerate() {
            if let Err(e) = msg {
                println!("Failed at index {}: {}", ix, e);
                assert!(false)
            }
        }
    }
}
