# Changelog

## 0.1.1

- A table cell's content ends before the one space or tab that pads its
  closing pipe: that separator is the pipe's, as an ATX closing sequence keeps
  the space before it. Further padding stays the cell's content. Render model
  schema 5, unchanged.

## 0.1.0

- First release of the parser libraries that `flark`'s build hook downloads:
  the `flark_parse` C ABI for macOS, iOS, Android, Linux and Windows, writing
  render model schema 5.
