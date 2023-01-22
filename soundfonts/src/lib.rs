pub mod sfz;

#[derive(Debug, PartialEq, Clone, Copy, Default)]
pub enum FilterType {
    LowPassPole,
    #[default]
    LowPass,
    HighPass,
    BandPass,
}
