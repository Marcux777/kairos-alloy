use crate::types::Bar;

pub trait MarketDataSource {
    fn next_bar(&mut self) -> Option<Bar>;
}

pub struct VecBarSource {
    bars: Vec<Bar>,
    index: usize,
}

impl VecBarSource {
    pub fn new(bars: Vec<Bar>) -> Self {
        Self { bars, index: 0 }
    }
}

impl MarketDataSource for VecBarSource {
    fn next_bar(&mut self) -> Option<Bar> {
        if self.index >= self.bars.len() {
            return None;
        }
        let bar = self.bars[self.index].clone();
        self.index += 1;
        Some(bar)
    }
}
