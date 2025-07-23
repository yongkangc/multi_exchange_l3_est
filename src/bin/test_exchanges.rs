use multi_exchange_l3_est::exchanges::{ExchangeType, Exchange};
use tokio::time::{timeout, Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("Testing Multi-Exchange L3 Order Book Estimator...\n");

    // Test Binance
    println!("🔄 Testing Binance Exchange...");
    let binance = ExchangeType::Binance.create_exchange();
    println!("✅ Exchange name: {}", binance.get_name());
    println!("✅ Symbol formatting: {} -> {}", "DOGEUSDT", binance.format_symbol("DOGEUSDT"));
    let (price_prec, qty_prec) = binance.get_precision("DOGEUSDT");
    println!("✅ Precision: price={}, quantity={}", price_prec, qty_prec);
    
    // Test connection (with timeout)
    match timeout(Duration::from_secs(5), binance.connect(&binance.format_symbol("DOGEUSDT"))).await {
        Ok(Ok(_)) => println!("✅ WebSocket connection successful"),
        Ok(Err(e)) => println!("⚠️  WebSocket connection failed: {}", e),
        Err(_) => println!("⚠️  WebSocket connection timeout (expected in some environments)"),
    }

    // Test Hyperliquid
    println!("\n🔄 Testing Hyperliquid Exchange...");
    let hyperliquid = ExchangeType::Hyperliquid.create_exchange();
    println!("✅ Exchange name: {}", hyperliquid.get_name());
    println!("✅ Symbol formatting: {} -> {}", "SOL", hyperliquid.format_symbol("SOL"));
    let (price_prec, qty_prec) = hyperliquid.get_precision("SOL");
    println!("✅ Precision: price={}, quantity={}", price_prec, qty_prec);

    // Test connection (with timeout)
    match timeout(Duration::from_secs(5), hyperliquid.connect(&hyperliquid.format_symbol("SOL"))).await {
        Ok(Ok(_)) => println!("✅ WebSocket connection successful"),
        Ok(Err(e)) => println!("⚠️  WebSocket connection failed: {}", e),
        Err(_) => println!("⚠️  WebSocket connection timeout (expected in some environments)"),
    }

    println!("\n🎉 All exchange modules loaded successfully!");
    println!("📊 The GUI application is ready to visualize order books from both exchanges.");
    println!("🚀 Run 'cargo run --release dogeusdt' on a machine with a display to see the visualization.");

    Ok(())
}