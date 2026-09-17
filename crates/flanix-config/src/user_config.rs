use crate::Config;
use std::io::{self, Write};

pub fn make_user_config() -> Config {
    let mut new_config = Config::default();
    println!("Use default config? (y/n)");
    let use_default = get_user_input().to_lowercase();
    if use_default.starts_with("y") {
        println!("Ok your config is ready to go!");
        return new_config;
    }

    println!("Enter database url:");
    new_config.database_url = get_user_input();
    println!("AWS endpoint:");
    new_config.aws_endpoint = get_user_input();
    println!("AWS access key id: ");
    new_config.aws_access_key_id = get_user_input();
    println!("AWS secret access key: ");
    new_config.aws_secret_access_key = get_user_input();
    println!("If you need more settings or what to change something, change the config file");

    new_config
}

pub fn get_user_input() -> String {
    print!("> ");
    io::stdout().flush().unwrap();
    let mut input = String::new();

    io::stdin()
        .read_line(&mut input)
        .expect("Failed to read line");

    let input = input.trim();

    input.to_string()
}
