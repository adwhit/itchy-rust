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

#[allow(unused_doc_comment)]
mod errors {
    error_chain!{
        foreign_links {
            Io(::std::io::Error);
        }
    }
}

pub struct Message;

pub fn parse_reader<R: Read>(reader: R) -> Result<Vec<Message>> {
    unimplemented!()
}

pub fn parse_gzip<P: AsRef<Path>>(path: P) -> Result<Vec<Message>> {
    let file = File::open(path)?;
    let reader = GzDecoder::new(file)?;
    parse_reader(reader)
}

pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<Vec<Message>> {
    let file = File::open(path)?;
    parse_reader(file)
}

#[derive(Debug, Clone)]
struct SystemEvent {
    tracking_number: u16,
    timestamp: u64,
    event_code: EventCode
}

#[derive(Debug, Clone, Copy)]
enum EventCode {
    StartOfMessages,
    StartOfSystemHours,
    StartOfMarketHours,
    EndOfMarketHours,
    EndOfSystemHours,
    EndOfMessages
}

named!(parse_system_event<SystemEvent>, do_parse!(
    length: be_u16 >> char!('S') >> tag!([0, 0]) >>
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
    (SystemEvent { tracking_number, timestamp, event_code })
));

#[inline]
pub fn be_u48(i: &[u8]) -> IResult<&[u8], u64> {
    if i.len() < 6 {
        IResult::Incomplete(Needed::Size(6))
    } else {
        let res =
            ((i[0] as u64) << 40) +
            ((i[1] as u64) << 32) +
            ((i[2] as u64) << 24) +
            ((i[3] as u64) << 16) +
            ((i[4] as u64) << 8) +
            i[5] as u64;
        IResult::Done(&i[6..], res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex_to_bytes(bytes: &[u8]) -> Vec<u8> {
        fn h2b(h: u8) -> Option<u8> {
            match h {
                v@b'0'...b'9' => Some(v - b'0'),
                v@b'a'...b'f' => Some(v - b'a' + 10),
                b' ' => None,
                _ => panic!("Invalid hex: {}", h as char)
            }
        }
        bytes.iter()
            .filter_map(|b| h2b(*b))
            .collect::<Vec<_>>()
            .chunks(2)
            .map(|slice| (slice[0] << 4) + slice[1])
            .collect()
    }

    #[test]
    fn system_event() {
        let code = b"000c 53 0000 0000 286aab3b3a99 4f";
        let bytes = hex_to_bytes(&code[..]);
        let (_, out) = parse_system_event(&bytes[..]).unwrap();
        println!("{:?}", out);
    }
}
