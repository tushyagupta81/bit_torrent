<div style="text-align: center;">

  # BitTorrent Client
  Minimal implementation of a BitTorrent Client following the [BitTorrent Protocol](https://www.bittorrent.org/beps/bep_0003.html)

  <img src="images/example1.png" alt="example 1" width="600" align="middle" />
  <hr width="500"/>
  <img src="images/example2.png" alt="example 2" width="600"/>
</div>

## Async implementation

- Uses Tokio for async capabilities
- Includes a TUI using ratatui
- Has a central manager that helps decide which peer should download which piece
- Uses Multiple Producer Single Consumer(mpsc) model for interaction between <u>central manager and peers</u> and <u>peers, central manager and UI</u>

## Sync implementation

- Uses a mutlithreading model
- Has a shared resources architecture

## Installation

```bash
git clone https://github.com/tushyagupta81/bit_torrent.git
cd bit_torrent
cargo run --bin async_torrent --release -- <path to torrent>
```
