#[macro_use]
extern crate error_chain;
#[macro_use]
extern crate nom;
extern crate flate2;
extern crate arrayvec;

use std::io::prelude::*;
use std::fs::File;
use std::path::Path;
use std::fmt;

use flate2::read::GzDecoder;
use nom::{be_u8, be_u16, be_u32, IResult, Needed};
use arrayvec::ArrayString;

use errors::*;
pub use enums::*;

const BUFSIZE: usize = 200;

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

pub struct MessageStream<R> {
    reader: R,
    buffer: Box<[u8; BUFSIZE]>,
    bufstart: usize,
    bufend: usize,
    bytes_read: usize,
    read_calls: u32,
    messages: u32,
}

impl<R> fmt::Debug for MessageStream<R> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "MessageStream {{ read_calls: {}, bytes_read: {}, buffer_pos: {}, messages: {} }}",
            self.read_calls,
            self.bytes_read,
            self.bytes_read - (self.bufend - self.bufstart),
            self.messages
        )
    }
}


impl<R: Read> MessageStream<R> {
    fn new(reader: R) -> MessageStream<R> {
        MessageStream {
            reader,
            buffer: Box::new([0; BUFSIZE]),
            bufstart: 0,
            bufend: 0,
            bytes_read: 0,
            read_calls: 0,
            messages: 0,
        }
    }

    fn fetch_more_bytes(&mut self) -> Result<usize> {
        self.read_calls += 1;
        if self.bufend == BUFSIZE {
            // we need more data from the reader
            // first, copy the remnants back to the beginning of the buffer
            // (this should only be a few bytes)
            assert!(self.bufstart as usize > BUFSIZE / 2); // safety check
            assert!(BUFSIZE - self.bufstart < 50); // extra careful check
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
                    self.bufstart = self.bufend - rest.len();
                    self.messages += 1;
                    return Some(Ok(msg));
                }
                Error(e) => return Some(Err(format!("Parse failed: {}", e).into())),
                Incomplete(_) => {
                    // fall through to below... necessary to appease borrow checker
                }
            }
        }
        match self.fetch_more_bytes() {
            Ok(0) => Some(Err("Unexpected EOF".into())),
            Ok(ct) => {
                self.bufend += ct;
                self.bytes_read += ct;
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

named!(char2bool<bool>, alt!(
    char!('Y') => {|_| true} |
    char!('N') => {|_| false}
));

named!(maybe_char2bool<Option<bool>>, alt!(
    char!('Y') => {|_| Some(true)} |
    char!('N') => {|_| Some(false)} |
    char!(' ') => {|_| None}
));

#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    header: MsgHeader,
    body: MessageBody
}

#[derive(Debug, Clone, PartialEq)]
struct MsgHeader {
    stock_locate: u16,
    tracking_number: u16,
    timestamp: u64
}

named!(parse_message_header<MsgHeader>, do_parse!(
    stock_locate: be_u16 >>
    tracking_number: be_u16 >>
    timestamp: be_u48 >>
    (MsgHeader { stock_locate, tracking_number, timestamp })
));


#[derive(Debug, Clone, PartialEq)]
pub enum MessageBody {
    SystemEvent(EventCode),
    //RegShoRestriction(RegShoRestriction),
    StockDirectory(StockDirectory),
    ParticipantPosition(MarketParticipantPosition),
    Unknown {
        length: u16,
        tag: char,
        content: Vec<u8>,
    },
}

named!(parse_message<Message>, do_parse!(
    length: be_u16 >>
    tag: be_u8 >>
    header: parse_message_header >>
    body: switch!(value!(tag),  // TODO is this 'value' call necessary?
                  b'S' => call!(parse_system_event) |
                  b'R' => map!(parse_stock_directory, |sd| MessageBody::StockDirectory(sd)) |
                  b'L' => map!(parse_participant_position, |pp| MessageBody::ParticipantPosition(pp)) |
                  other => map!(take!(length - 1),
                                |slice| MessageBody::Unknown {
                                    length,
                                    tag: other as char,
                                    content: Vec::from(slice)
                                })) >>
    (Message { header, body })
));


#[derive(Debug, Clone, PartialEq)]
pub struct StockDirectory {
    stock: ArrayString<[u8; 8]>,
    market_category: MarketCategory,
    financial_status: FinancialStatus,
    round_lot_size: u32,
    round_lots_only: bool,
    issue_classification: IssueClassification,
    issue_subtype: IssueSubType,
    authenticity: bool,
    short_sale_threshold: Option<bool>,
    ipo_flag: Option<bool>,
    luld_ref_price_tier: LuldRefPriceTier,
    etp_flag: Option<bool>,
    etp_leverage_factor: u32,
    inverse_indicator: bool,
}

named!(parse_system_event<MessageBody>, do_parse!(
    event_code: alt!(
        char!('O') => { |_| EventCode::StartOfMessages } |
        char!('S') => { |_| EventCode::StartOfSystemHours } |
        char!('Q') => { |_| EventCode::StartOfMarketHours } |
        char!('M') => { |_| EventCode::EndOfMarketHours } |
        char!('E') => { |_| EventCode::EndOfSystemHours } |
        char!('C') => { |_| EventCode::EndOfMessages }
    ) >>
    (MessageBody::SystemEvent(event_code))
));

named!(parse_stock_directory<StockDirectory>, do_parse!(
    stock: map!(take_str!(8), |s| ArrayString::from(s).unwrap()) >>
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

    // FIXME these are dummy values
    issue_classification: value!(IssueClassification::Unit, take!(1)) >>
    issue_subtype: value!(IssueSubType::AlphaIndexETNs, take!(2)) >>
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
    mpid: ArrayString<[u8; 4]>,
    stock: ArrayString<[u8; 8]>,
    primary_market_maker: bool,
    market_maker_mode: MarketMakerMode,
    market_participant_state: MarketParticipantState
}

named!(parse_participant_position<MarketParticipantPosition>, do_parse!(
    mpid: map!(take_str!(4), |s| ArrayString::from(s).unwrap()) >>
    stock: map!(take_str!(8), |s| ArrayString::from(s).unwrap()) >>
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

#[cfg(test)]
mod tests {
    use super::*;

    // for pretty-printing
    fn bytes2hex(bytes: &[u8]) -> Vec<(char, char)> {
        fn b2h(b: u8) -> (char, char) {
            let left = match b >> 4 {
                v@0...9 => b'0' + v,
                v@10...15 => b'a' + v - 10,
                _ => unreachable!()
            };
            let right = match b & 0xff {
                v@0...9 => b'0' + v,
                v@10...15 => b'a' + v - 10,
                _ => unreachable!()
            };
            (left as char, right as char)
        }
        bytes.iter().map(|b| b2h(*b)).collect()
    }

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
        let (_, out) = parse_system_event(&bytes[..]).unwrap();
        if let MessageBody::SystemEvent(EventCode::StartOfMessages) = out {
        } else {
            panic!("Expected SystemEvent,  found {:?}", out)
        }
    }

    #[test]
    fn stock_directory() {
        let code = b"41 2020 2020 2020 204e 2000
                     0000 644e 435a 2050 4e20 314e 0000 0000 4e";
        let bytes = hex_to_bytes(&code[..]);
        parse_stock_directory(&bytes[..]).unwrap();
    }

    #[test]
    fn market_participant_position() {
        let code = b"41 44 41 4d 42 42 52 59 20 20 20 20 59 4e 41";
        let bytes = hex_to_bytes(&code[..]);
        parse_participant_position(&bytes[..]).unwrap();
    }

    #[test]
    fn full_parse() {
        let iter = parse_file("data/01302016.NASDAQ_ITCH50").unwrap();
        for (ix, msg) in iter.enumerate() {
            if let Err(e) = msg {
                panic!("Mesaage {} failed to parse: {}", ix, e);
            } else {
                if let MessageBody::Unknown { tag, content, .. } = msg.unwrap().body {
                    print!("Message {} tag '{}' unknown: [", ix, tag);
                    for v in content { print!("{:02x} ", v) }
                    println!("]");
                    panic!()
                }
            }
        }
    }
}
