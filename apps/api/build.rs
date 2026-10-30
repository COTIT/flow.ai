//! Rebuild when migrations change: `sqlx::migrate!` embeds them at compile time,
//! and a migration-only change would otherwise ship a binary without it.
fn main() {
    println!("cargo:rerun-if-changed=migrations");
}
