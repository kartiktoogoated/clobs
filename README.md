# orderbooks-superdevs


# matching engine adding orders logic
Incoming Order → Validate + Lock Funds
              ↓
          Check Opposite Book
              ↓
    Match at Each Price Level
              ↓
    Fill until remaining == 0
              ↓
Store Fill Results + Add Remainder to Book (if needed)
              ↓
Return FillResult
