# Multi-Exchange L3 Order Book Estimator

![Demo GIF](demo.gif)

This project is a real-time visualization tool for order books from multiple cryptocurrency exchanges. The tool uses L2 data's change in time to naively estimate a L3 order book microstructure. We can change to more complex models to estimate the L3 book later.

## Supported Exchanges

* **Binance**: Perpetual swap markets
* **Hyperliquid**: Perpetual markets

## Features

* **Multi-Exchange Support**: Switch between Binance and Hyperliquid in real-time
* **Real-time Data**: Streams order book data using WebSocket APIs
* **Bid/Ask Visualization**: Displays the current bids and asks with dynamic visualization
* **Order Queue Estimation**: Estimates the order queue at each price level using L2 data
* **Dynamic Bar Coloring**: Bid and ask bars are dynamically colored based on the age of the order
* **K-means Clustering**: Optional clustering mode to analyze order patterns

## Usage

#### From source
To try the project, you'll need to have Rust installed on your system. You can install it from [https://www.rust-lang.org/](https://www.rust-lang.org/).

1. Clone the repository:

```bash
git clone https://github.com/yongkangc/multi_exchange_l3_est.git
cd multi_exchange_l3_est
```

2. Run with a trading pair:

For Binance (default):
```bash
cargo run -r dogeusdt
```

For Hyperliquid, switch exchanges in the UI:
```bash
cargo run -r SOL
```

#### From release binary

Go to https://github.com/yongkangc/multi_exchange_l3_est/releases and download the newest release binary.

The chart dynamically updates as new WebSocket messages are received, and the bars for bids and asks are color-coded based on the order age.

#### P.S. You need enough time to wait for the estimator to start working based on the history L2 data.

## Controls

- **Exchange Dropdown**: Switch between Binance and Hyperliquid
- **Symbol Input**: Change the trading pair (e.g., `dogeusdt` for Binance, `SOL` for Hyperliquid)
- **Toggle K-Means Mode**: Enable/disable order clustering visualization
- **Batch Size/Max Iter**: Adjust K-means clustering parameters (when enabled)

## Architecture

The project uses a modular exchange abstraction:

- `src/exchanges/mod.rs` - Common exchange interface and data structures
- `src/exchanges/binance.rs` - Binance-specific implementation
- `src/exchanges/hyperliquid.rs` - Hyperliquid-specific implementation
- `src/main.rs` - GUI application and order book visualization
- `src/kmeans.rs` - K-means clustering for order analysis

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---

## Todo

- [x] Add multi-exchange support (Binance + Hyperliquid)
- [x] Add exchange selection dropdown
- [x] Implement Hyperliquid WebSocket integration
- [x] Add Cluster Algo on orders and display them with different color
- [ ] Add more exchanges (Bybit, OKX, etc.)
- [ ] Improve L3 estimation algorithms
- [ ] Add order flow analysis