pub mod sfz;

#[derive(Debug, Clone, Copy)]
pub enum CutoffPassCount {
    One,
    Two,
    Four,
    Six,
}

#[derive(Debug, Clone, Copy)]
pub enum FilterType {
    LowPass { passes: CutoffPassCount },
    HighPass { passes: CutoffPassCount },
}

impl Default for FilterType {
    fn default() -> Self {
        FilterType::LowPass {
            passes: CutoffPassCount::Two,
        }
    }
}
