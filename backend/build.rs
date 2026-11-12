fn main() {
    // sqlx embeds migrations at compile time; directory changes must rebuild it.
    println!("cargo:rerun-if-changed=migrations");
}
