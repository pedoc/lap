/**
 * Common constants and shared types
 */

// Image support
pub const NORMAL_IMGS: &[&str] = &[
    "jpg", "jpeg", "jfif", "png", "gif", "bmp", "tif", "tiff", "webp", "avif", "heic", "heif", "hif", "jxl",
];

// Image formats decoded through the bundled FFmpeg sidecar.
pub const FFMPEG_BACKED_IMGS: &[&str] = &[
    "psd", // Photoshop native format; preview only (merged composite), no edit support
    "exr", // HDR industry standard (VFX/rendering/Resolve output)
    "hdr", "rgbe", // Radiance RGBE format; HDR panoramas and tonemapped output
    "tga",  // Legacy format; some cameras/scanners/game asset pipelines
    "dds",  // DirectDraw Surface; game texture format; relevant only for game-dev users
    "qoi",  // Fast lossless format (2021); ecosystem still immature
    "jp2", "j2k", "j2c", "jpc", "jpf", "jpx", // JPEG 2000 family; medical/satellite use only
    "dpx", // Digital cinema intermediate format; niche film/grading pipeline only
];

// RAW support
pub const RAW_IMGS: &[&str] = &[
    "cr2", "cr3", "crw", // Canon
    "nef", "nrw", // Nikon
    "arw", "srf", "sr2", // Sony
    "raf", // Fujifilm
    "rw2", // Panasonic
    "orf", // Olympus / OM System
    "pef", // Pentax
    "dng", // Adobe / generic RAW
    "srw", // Samsung
    "rwl", // Leica
    "mrw", // Minolta / Konica Minolta
    "3fr", // Hasselblad
    "mos", "iiq", // Leaf / Phase One
    // "x3f", // Sigma / Foveon - temporarily disabled: current LibRaw path reports FileUnsupported for sampled X3F files, so indexing fails at RAW dimensions.
    "dcr", "kdc", // Kodak
    "erf", // Epson
    "mef", // Mamiya
    "raw", // Generic vendor RAW extension
    "mdc", // Legacy RAW variant in sample set
];

// Video support
pub const VIDEOS: &[&str] = &[
    "mpg", "mpeg", "mp4", "mkv", "avi", "mov", "webm", "flv", "wmv", "3gp", "m4v", "hevc", "asf",
    "mts", "m2ts", "mod", "tod", "ts",
];
// pub const AUDIOS: &[&str] = &[
//     "mp3", "wav", "flac", "aac", "m4a", "ogg", "wma", "mp2", "mp1", "ape", "alac", "wavpack",
// ];
