use bitflags::bitflags;

#[derive(Debug, Clone)]
pub struct SingleWindowDefinition {
    pub window_id: u8,
    pub horizontal_pos: u16,
    pub vertical_pos: u16,
    pub width: u16,
    pub height: u16,
}

#[derive(Debug, Clone)]
pub struct PaletteEntry {
    pub palette_entry_id: u8,
    pub luminance: u8,
    pub color_diff_red: u8,
    pub color_diff_blue: u8,
    pub transparency: u8,
}

#[derive(Debug, Clone)]
pub struct PresentationComposition {
    pub width: u16,
    pub height: u16,
    pub frame_rate: u8,
    pub composition_number: u16,
    pub composition_state: CompositionState,
    pub palette_update_flag: bool,
    pub palette_id: u8,
    pub composition_objects: Vec<CompositionObject>,
}

#[derive(Debug, Clone)]
pub struct ObjectDefinition {
    pub object_id: u16,
    pub object_version: u8,
    pub last_in_sequence: LastInSequence,
    pub width: u16,
    pub height: u16,
    pub rle_data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct PaletteDefinition {
    pub palette_id: u8,
    pub palette_version: u8,
    pub entries: Vec<PaletteEntry>,
}

bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct LastInSequence: u8 {
        const FIRST_IN_SEQUENCE = 0b01000000;
        const LAST_IN_SEQUENCE  = 0b10000000;
    }
}

#[derive(Debug, Clone)]
pub struct CompositionObject {
    pub object_id: u16,
    pub window_id: u8,
    pub object_cropped_flag: bool,
    pub object_horizontal_pos: u16,
    pub object_vertical_pos: u16,
    pub object_cropping_horizontal_pos: u16,
    pub object_cropping_vertical_pos: u16,
    pub object_cropping_width: u16,
    pub object_cropping_height: u16,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub enum CompositionState {
    Normal,
    AcquisitionPoint,
    EpochStart,
}

#[derive(Debug, Clone)]
pub struct PgsDisplaySet {
    pub pcs: PresentationComposition,
    pub wds: Vec<SingleWindowDefinition>,
    pub pds: Vec<PaletteDefinition>,
    pub ods: Vec<ObjectDefinition>,
}
