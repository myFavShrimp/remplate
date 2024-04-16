# remplate

Compile-time templating that feels Rust-native

## Usage

```rust
// my_template.html

{
    let title = "My Awesome Template";
    let paragraph = "Lorem ipsum etc.";
}

<h1>{ title }</h1>

<p>{paragraph}</p>

{
    let debug_info = if debug_enabled {
        Some("debugging is enabled")
    } else {
        None
    };

    debug_info:?
}
```

```rust
// src/main.rs

fn main() {
    println!("{}", remplate::remplate!("my_template.html"));
}
```

```shell
~/remplate-example: cargo run
   Compiling remplate-example v0.1.0 (/home/user/remplate-example)
    Finished dev [unoptimized + debuginfo] target(s) in 0.13s
     Running `target/debug/remplate-example`


<h1>My Awesome Template</h1>

<p>Lorem ipsum etc.</p>

Some("debugging is enabled")
```
