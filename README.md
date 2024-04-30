# remplate

Templating that feels Rust-native

## Features

- regular Rust syntax in code blocks
- support for `format!`-macro syntax

## Usage

```rust
// my_template.html

{
    let title = "My Awesome Template";
    let paragraph = "Lorem ipsum etc.";
}

<h1>{ title }</h1>

<p>{ paragraph }</p>

{
    let debug_info = if self.debug_enabled {
        Some("debug is enabled")
    } else {
        None
    };

    debug_info:?
}
```

```rust
// src/main.rs

#[derive(remplate::Remplate)]
#[remplate(path = "my_template.html")]
struct MyTemplate {
    debug_enabled: bool,
}


fn main() {
    println!(
        "{}",
        MyTemplate {
            debug_enabled: true
        }
    );
}
```

```html
~/remplate-example: cargo run
   Compiling remplate-example v0.1.0 (/home/user/remplate-example)
    Finished dev [unoptimized + debuginfo] target(s) in 0.13s
     Running `target/debug/remplate-example`


<h1>My Awesome Template</h1>

<p>Lorem ipsum etc.</p>

Some("debug is enabled")
```
