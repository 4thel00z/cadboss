#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let Ok(doc) = cadboss_dxf::load(data) else {
        return;
    };
    std::hint::black_box(cadboss_output::to_markdown(&doc, "fuzz"));
    std::hint::black_box(cadboss_output::to_json(&doc));
});
