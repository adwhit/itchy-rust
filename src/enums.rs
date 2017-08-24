use nom::{IResult, be_u8};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventCode {
    StartOfMessages,
    StartOfSystemHours,
    StartOfMarketHours,
    EndOfMarketHours,
    EndOfSystemHours,
    EndOfMessages,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketCategory {
    NasdaqGlobalSelect,
    NasdaqGlobalMarket,
    NasdaqCaptialMarket,
    Nyse,
    NyseMkt,
    NyseArca,
    BatsZExchange,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinancialStatus {
    Normal,
    Deficient,
    Delinquent,
    Bankrupt,
    Suspended,
    DeficientBankrupt,
    DeficientDelinquent,
    DelinquentBankrupt,
    DeficientDelinquentBankrupt,
    EtpSuspended,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueClassification {
    AmericanDepositaryShare,
    Bond,
    CommonStock,
    DepositoryReceipt,
    A144,
    LimitedPartnership,
    Notes,
    OrdinaryShare,
    PreferredStock,
    OtherSecurities,
    Right,
    SharesOfBeneficialInterest,
    ConvertibleDebenture,
    Unit,
    UnitsPerBenifInt,
    Warrant,
}

pub(crate) fn parse_issue_classification(input: &[u8]) -> IResult<&[u8], IssueClassification> {
    map_opt!(input, be_u8, |v| {
        use IssueClassification::*;
        Some(match v {
            b'A' => AmericanDepositaryShare,
            b'B' => Bond,
            b'C' => CommonStock,
            b'F' => DepositoryReceipt,
            b'I' => A144,
            b'L' => LimitedPartnership,
            b'N' => Notes,
            b'O' => OrdinaryShare,
            b'P' => PreferredStock,
            b'Q' => OtherSecurities,
            b'R' => Right,
            b'S' => SharesOfBeneficialInterest,
            b'T' => ConvertibleDebenture,
            b'U' => Unit,
            b'V' => UnitsPerBenifInt,
            b'W' => Warrant,
            _ => return None,
        })
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IssueSubType {
    PreferredTrustSecurities,
    AlphaIndexETNs,
    IndexBasedDerivative,
    CommonShares,
    CommodityBasedTrustShares,
    CommodityFuturesTrustShares,
    CommodityLinkedSecurities,
    CommodityIndexTrustShares,
    CollateralizedMortgageObligation,
    CurrencyTrustShares,
    CommodityCurrencyLinkedSecurities,
    CurrencyWarrants,
    GlobalDepositaryShares,
    ETFPortfolioDepositaryReceipt,
    EquityGoldShares,
    ETNEquityIndexLinkedSecurities,
    ExchangeTradedManagedFunds,
    ExchangeTradedNotes,
    EquityUnits,
    Holdrs,
    ETNFixedIncomeLinkedSecurities,
    ETNFuturesLinkedSecurities,
    GlobalShares,
    ETFIndexFundShares,
    InterestRate,
    IndexWarrant,
    IndexLinkedExchangeableNotes,
    CorporateBackedTrustSecurity,
    ContingentLitigationRight,
    Llc,
    EquityBasedDerivative,
    ManagedFundShares,
    ETNMultiFactorIndexLinkedSecurities,
    ManagedTrustSecurities,
    NYRegistryShares,
    OpenEndedMutualFund,
    PrivatelyHeldSecurity,
    PoisonPill,
    PartnershipUnits,
    ClosedEndFunds,
    RegS,
    CommodityRedeemableCommodityLinkedSecurities,
    ETNRedeemableFuturesLinkedSecurities,
    CommodityRedeemableCurrencyLinkedSecurities,
    Seed,
    SpotRateClosing,
    SpotRateIntraday,
    TrackingStock,
    TrustCertificates,
    TrustUnits,
    Portal,
    ContingentValueRight,
    TrustIssuedReceipts,
    WorldCurrencyOption,
    Trust,
    Other,
    NotApplicable,
}

pub(crate) fn parse_issue_subtype(input: &[u8]) -> IResult<&[u8], IssueSubType> {
    map_opt!(input, take!(2), |v: &[u8]| {
        use IssueSubType::*;
        Some(match v {
            b"A " => PreferredTrustSecurities,
            b"AI" => AlphaIndexETNs,
            b"B " => IndexBasedDerivative,
            b"C " => CommonShares,
            b"CB" => CommodityBasedTrustShares,
            b"CF" => CommodityFuturesTrustShares,
            b"CL" => CommodityLinkedSecurities,
            b"CM" => CommodityIndexTrustShares,
            b"CO" => CollateralizedMortgageObligation,
            b"CT" => CurrencyTrustShares,
            b"CU" => CommodityCurrencyLinkedSecurities,
            b"CW" => CurrencyWarrants,
            b"D " => GlobalDepositaryShares,
            b"E " => ETFPortfolioDepositaryReceipt,
            b"EG" => EquityGoldShares,
            b"EI" => ETNEquityIndexLinkedSecurities,
            b"EM" => ExchangeTradedManagedFunds,
            b"EN" => ExchangeTradedNotes,
            b"EU" => EquityUnits,
            b"F " => Holdrs,
            b"FI" => ETNFixedIncomeLinkedSecurities,
            b"FL" => ETNFuturesLinkedSecurities,
            b"G " => GlobalShares,
            b"I " => ETFIndexFundShares,
            b"IR" => InterestRate,
            b"IW" => IndexWarrant,
            b"IX" => IndexLinkedExchangeableNotes,
            b"J " => CorporateBackedTrustSecurity,
            b"L " => ContingentLitigationRight,
            b"LL" => Llc,
            b"M " => EquityBasedDerivative,
            b"MF" => ManagedFundShares,
            b"ML" => ETNMultiFactorIndexLinkedSecurities,
            b"MT" => ManagedTrustSecurities,
            b"N " => NYRegistryShares,
            b"O " => OpenEndedMutualFund,
            b"P " => PrivatelyHeldSecurity,
            b"PP" => PoisonPill,
            b"PU" => PartnershipUnits,
            b"Q " => ClosedEndFunds,
            b"RC" => RegS,
            b"RF" => CommodityRedeemableCommodityLinkedSecurities,
            b"RT" => ETNRedeemableFuturesLinkedSecurities,
            b"RU" => CommodityRedeemableCurrencyLinkedSecurities,
            b"S " => Seed,
            b"SC" => SpotRateClosing,
            b"SI" => SpotRateIntraday,
            b"T " => TrackingStock,
            b"TC" => TrustCertificates,
            b"TU" => TrustUnits,
            b"U " => Portal,
            b"V " => ContingentValueRight,
            b"W " => TrustIssuedReceipts,
            b"WC" => WorldCurrencyOption,
            b"X " => Trust,
            b"Y " => Other,
            b"Z " => NotApplicable,
            _ => return None,
        })
    })
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LuldRefPriceTier {
    Tier1,
    Tier2,
    Na,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketMakerMode {
    Normal,
    Passive,
    Syndicate,
    Presyndicate,
    Penalty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketParticipantState {
    Active,
    Excused,
    Withdrawn,
    Suspended,
    Deleted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegShoAction {
    None,
    Intraday,
    Extant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradingState {
    Halted,
    Paused,
    QuotationOnly,
    Trading,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImbalanceDirection {
    Buy,
    Sell,
    NoImbalance,
    InsufficientOrders,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrossType {
    Opening,
    Closing,
    IpoOrHalted,
    Intraday,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpoReleaseQualifier {
    Anticipated,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LevelBreached {
    L1,
    L2,
    L3,
}
