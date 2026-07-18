# Heading Level One

The quick brown fox jumps over the lazy dog. Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris.

## Heading Level Two

A paragraph with **bold text**, *italic text*, ***bold italic***, `inline code`, ~~strikethrough~~, and a [link to example](https://example.com). Autolink: <https://rust-lang.org>.

### Heading Three

Setext-style heading right below:

Setext H2
---------

And a paragraph under it.

## Blockquotes

Single line:

> Diplomacy is the art of saying "nice doggie" until you can find a rock.

Nested two levels:

> Outer quote line.
>
> > Inner quote, deeper meaning.
> >
> > With a **bold** bit inside.

## Lists

Ordered:

1. First ordered item.
2. Second item with `inline code`.
3. Third item with a [link](https://example.com).
   1. Nested ordered sub-item.
   2. Another nested item.
4. Fourth item.

Unordered:

- Bullet alpha.
- Bullet beta with **bold**.
  - Nested bullet one.
  - Nested bullet two.
- Bullet gamma.

Task list:

- [ ] Todo item unchecked.
- [x] Done item checked.

## Code Blocks

Fenced with language (syntax highlighted via Vesper 24-bit):

```rust
use std::collections::HashMap;

/// A short example function.
fn main() {
    let mut map: HashMap<String, i32> = HashMap::new();
    map.insert("answer".to_string(), 42);
    for (k, v) in &map {
        println!("{k} = {v}");
    }
    let xs: Vec<u8> = (0..10).filter(|n| n % 2 == 0).collect();
    println!("{xs:?}");
}
```

Fenced with another language:

```go
package main

import "fmt"

func main() {
    nums := []int{1, 2, 3}
    for i, n := range nums {
        fmt.Printf("%d: %d\n", i, n)
    }
}
```

Fenced, no language:

```
plain code block
no language
multiple lines
```

Indented code block:

    func indented() {
        // old-style indented code
    }

## Horizontal Rule & Table

---

| Feature        | Status | Notes                          |
|----------------|:------:|--------------------------------|
| CommonMark     |   ✓    | Full spec support              |
| GFM tables     |   ✓    | Pipe tables, aligned columns   |
| Task lists     |   ✓    | `- [ ]` and `- [x]`            |
| Strikethrough  |   ✓    | `~~text~~`                     |
| Autolinks      |   ✓    | `<https://...>`                |

A longer table with more columns to stress wrapping:

| Name            | Lang   | Stars | License | Notes                       |
|-----------------|--------|------:|---------|-----------------------------|
| mate-rs         | Rust   |   142 | MIT     | This very project           |
| mdcat           | Rust   |  2100 | MPL2    | Source of the renderer      |
| pulldown-cmark  | Rust   |  1900 | MIT     | The parser underneath       |
| ratatui         | Rust   | 11000 | MIT     | Terminal UI framework       |

## Inline edge cases

Nested emphasis: `**bold _and italic_ together**` renders as **bold _and italic_ together**.

Code span with special chars: `let x = vec![1, 2, 3];`.

Multiple links in one line: [first](https://a.example), [second](https://b.example), [third](https://c.example).

Backslash escapes: \*not bold\*, \_not italic\_, \[not a link\](not-a-url).

## Footnotes

A footnote reference[^1] and another[^note].

[^1]: This is the first footnote.
[^note]: Second footnote with **bold** inside.

## Hard line break

Line ending with two trailing spaces:
this should be a new line, same paragraph.

## GFM Alerts

> [!NOTE]
> This is a GFM NOTE alert. Useful for informational callouts.

> [!TIP]
> A helpful TIP alert.

> [!IMPORTANT]
> Something you really must read.

> [!WARNING]
> Be careful with this one.

> [!CAUTION]
> Danger zone.

## Definition Lists (GFM)

Apple
: A fruit and a technology company.

Banana
: Just a fruit, really.

Cherry
: Small, red, and tart.
: Also delicious in pies.

## Long paragraph (wrapping stress test)

Lorem ipsum dolor sit amet, consectetur adipiscing elit. Sed do eiusmod tempor incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat. Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in culpa qui officia deserunt mollit anim id est laborum.

## Heading with `inline code`

Nested headings with inline markup also work.

### Final heading

End of document.
