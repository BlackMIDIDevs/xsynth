pub mod sfz;

#[derive(Debug, Clone, Copy)]
pub enum FilterType {
    LowPole,
    HighPole,
    ButterworthFilter,
}

impl Default for FilterType {
    fn default() -> Self {
        FilterType::ButterworthFilter
    }
}
