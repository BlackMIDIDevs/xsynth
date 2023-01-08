pub mod sfz;

#[derive(Debug, PartialEq, Clone, Copy)]
pub enum FilterType {
    LowPassPole,
    LowPass,
    HighPass,
    BandPass,
}

impl Default for FilterType {
    fn default() -> Self {
        FilterType::LowPass
    }
}
