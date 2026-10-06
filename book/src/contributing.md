# Contributing

EvoCC welcomes help. You can report a bug, send a reduced test case, improve this book or fix a
defect in the compiler.

- [CONTRIBUTING.md](https://github.com/onehr/evocc/blob/main/CONTRIBUTING.md) explains how to build
  EvoCC and how a change moves from a failing test to a commit.
- [TODO.md](https://github.com/onehr/evocc/blob/main/TODO.md) is the task tracker. Each task has a
  stable name, a status and the evidence that closes it.
- The [issue templates](https://github.com/onehr/evocc/issues/new/choose) ask for what we need to
  reproduce a bug, a wrong-code result or a refused proof.

## Work on this book

The source of this book is in `book/src/`, one Markdown file for each chapter. `book/src/SUMMARY.md`
lists the chapters. The book is built with [mdBook](https://rust-lang.github.io/mdBook/). The book
workflow uses mdBook 0.4.52:

```console
$ mdbook build book
$ mdbook serve book --open
```

`mdbook build` writes the HTML into `book/book/`. `mdbook serve` builds the book, opens it in your
browser and builds it again when you save a file.

Each command in this book must be one that you ran, and each output must be the output that you
saw. If you cannot run a command, do not invent its output.

## License

EvoCC and this book are licensed under the Apache License 2.0. Copyright 2026 Haoran Wang,
NexRidge. A contribution that you submit is licensed under the same license.
