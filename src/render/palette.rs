pub const PALETTE: [(u8, u8, u8); 8] = [
    (86, 182, 230),
    (66, 118, 240),
    (172, 92, 226),
    (222, 76, 178),
    (226, 72, 72),
    (238, 146, 60),
    (232, 200, 72),
    (122, 196, 80),
];

pub fn rgb(color: usize) -> (u8, u8, u8) {
    PALETTE[color % PALETTE.len()]
}
