pub enum CCTypes {
    
}

pub struct MSBLSBControl {
    msb: u8,
    lsb: u8,
    value: f64,
}

impl MSBLSBControl {
    
}

pub enum CCChangeType {
    MSB(u8),
    LSB(u8),
    Value(f32),
}
