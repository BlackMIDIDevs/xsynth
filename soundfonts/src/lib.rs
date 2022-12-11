pub mod sfz;

#[derive(Debug, Clone, Copy)]
pub enum FilterType {
    LowPass { passes: usize },
    HighPass { passes: usize },
}

impl Default for FilterType {
    fn default() -> Self {
        FilterType::LowPass { passes: 2 }
    }
}
