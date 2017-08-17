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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LuldRefPriceTier {
    Tier1,
    Tier2,
    Na,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketMakerMode {
    Normal,
    Passive,
    Syndicate,
    Presyndicate,
    Penalty
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MarketParticipantState {
    Active,
    Excused,
    Withdrawn,
    Suspended,
    Deleted
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegShoAction {
    None,
    Intraday,
    Extant,
}
