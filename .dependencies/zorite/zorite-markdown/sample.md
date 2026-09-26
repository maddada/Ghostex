# Markdown coverage

A reference document exercising everything `zorite-markdown` renders. Seed it into
a Zorite database (it's the `Markdown Test` page) to eyeball the renderer, or
read it as living documentation of supported syntax.

## Headings

# Heading 1
## Heading 2
### Heading 3
#### Heading 4
##### Heading 5
###### Heading 6

## Inline styles

Normal text, **bold**, *italic*, ***bold italic***, ~~strikethrough~~, and
`inline code`. They combine: **bold with `code`**, *italic with [a link](https://www.gpui.rs)*,
and ~~struck **bold**~~.

A line with a hard break\
continues on the next line.

## Links & navigation

- External link: [GPUI](https://www.gpui.rs)
- Autolink: <https://commonmark.org>
- Reference-style link: [the CommonMark spec][cm]
- Wiki-link (Zorite): [[project]]
- Tags (Zorite): #markdown #test-page

[cm]: https://commonmark.org

## Lists

Unordered, nested:

- Fruit
  - Apple
    - Granny Smith
  - Pear
- Vegetables

Ordered, nested, custom start:

3. Third
4. Fourth
   1. Nested first
   2. Nested second
5. Fifth

Task list (GFM):

- [x] Render Markdown
- [x] Resize images
- [ ] Take over the world

## Blockquotes

> A single-level quote with **emphasis**.
>
> > A nested quote.
>
> Back to the first level.

## Code

Inline: `cargo run`. A fenced block with a language:

```rust
fn main() {
    println!("hello, Zorite");
}
```

A fenced block without a language:

```
plain preformatted text
  indentation preserved
```

## Table

| Feature   | Status |        Notes |
| --------- | :----: | -----------: |
| Headings  |   ok   |       h1–h6  |
| Tables    |   ok   |   alignment  |
| Footnotes |   ok   | refs + defs  |

## Image

A standalone image with an explicit width (drag the corner handle to resize):

![Sample image](images/istockphoto-1381637603-612x612.jpg){width=420}

## Footnotes

A statement that wants a source[^1] and another with a named reference.[^note]

[^1]: The first footnote.
[^note]: The second footnote, with **formatting** and a [link](https://example.com).

## Raw HTML

Inline <mark>highlighted</mark> text, and a block:

<div class="callout">Raw HTML is shown literally, never executed.</div>

## Alerts

> [!NOTE]
> Useful information that users should know.

> [!TIP]
> Helpful advice for doing things better.

> [!IMPORTANT]
> Key information users need to know.

> [!WARNING]
> Urgent info that needs immediate attention.

> [!CAUTION]
> Advises about risks or negative outcomes.

> A plain blockquote, for contrast.

## Thematic break

---

That's the full set.
