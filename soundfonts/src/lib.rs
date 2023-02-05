pub mod sfz;

#[derive(Debug, PartialEq, Clone, Copy, Default)]
pub enum FilterType {
    LowPassPole,
    #[default]
    LowPass,
    HighPass,
    BandPass,
}

#[derive(Debug, Clone)]
pub enum LoopMode {
    NoLoop,
    OneShot,
    LoopContinuous,
    LoopSustain,
}
