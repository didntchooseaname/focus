# Integer limits, an invented page

{{#include ../shared/banner.md}}

## Examples

{{#tabs }}
{{#tab name="Rust" }}
```rust
fn main() {
    let x: u8 = 255;
    println!("{}", x.wrapping_add(1));
}
```
{{#endtab }}

{{#tab name='C'}}
A short sentence, then code:

```c
unsigned char x = 255;
x = x + 1;
```
{{#endtab}}
{{#endtabs}}

## Related pages

{{#ref}}
../topics/overflow.md
{{#endref}}

A sentence with {{#ref}}../topics/wrapping.md{{#endref}} inside it.

- A list item
  {{#ref}}
  ../topics/lists.md
  {{#endref}}

{{#file}}
sample.bin
{{#endfile}}

### Nested groups

{{#tabs}}
{{#tab name="Outer"}}
{{#tabs}}
{{#tab name="Inner"}}
deep text
{{#endtab}}
{{#endtabs}}
     {{#endtab }}
     {{#endtabs }}

{{#include ../shared/banner.md}}
