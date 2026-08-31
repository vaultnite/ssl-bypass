// injects build timestamp similar to og aurora

use time::OffsetDateTime;
use time::format_description::parse_borrowed;

fn main() {
    let format = parse_borrowed::<3>(
        "[month repr:short] [day padding:zero] [year] at [hour]:[minute]:[second]",
    )
    .expect("valid format description");

    println!(
        "cargo:rustc-env=BUILD_TIMESTAMP={}",
        OffsetDateTime::now_utc()
            .format(&format)
            .expect("formatting failed")
    );
    println!("cargo:rerun-if-changed=build.rs");
}
