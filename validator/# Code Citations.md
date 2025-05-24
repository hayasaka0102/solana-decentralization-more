# Code Citations


## License: Apache-2.0
https://github.com/solana-labs/solana/blob/27eff8408b7223bb3c4ab70523f8a8dca3ca6645/runtime/src/bank_forks.rs

```
root_bank.slot();

        let mut banks = HashMap::new();
        banks.insert(
            root_slot,
            BankWithScheduler::new_without_scheduler(root_bank.clone()),
        );

        let parents = root_bank.parents();
        for parent in parents {
            if banks
                .insert(
                    parent.slot(),
                    BankWithScheduler::new_without_scheduler(parent.clone()),
                )
                .is_some()
            {
                // All ancestors have already been inserted by another fork
                break;
            }
        }

        let mut descendants =
```
