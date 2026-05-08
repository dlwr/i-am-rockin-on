#[cfg(feature = "ssr")]
fn main() {
    println!("scrape CLI placeholder");
}

#[cfg(not(feature = "ssr"))]
fn main() {}
