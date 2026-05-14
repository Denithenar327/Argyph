fn parse_config(input: &str) -> Option<String> {
    input.lines().find(|l| l.starts_with("name=")).map(|l| l.to_string())
}
fn main() { println!("{:?}", parse_config("name=demo")); }
