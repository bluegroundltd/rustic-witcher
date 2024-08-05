**Validation Config**
=====================

This configuration defines a set of validations to be applied to data after the data export flow.

### Example

```toml
[[validations]]
table = "table1"
query = "select * from public.table1 limit 5"
column_to_check = "column1"
[validations.value_check_type]
type = "Contains" # or "Equals"
value = "a_value"
```

### Table Selection
-------------------
TOML key: `[[validations]]`

* **table**: The target table for validation.
* **query**: The SQL query which will be executed to retrieve a limited subset of data from the table.
* **column_to_check**: The specific column being validated.

### Column Validation
---------------------
TOML key: `[validations.value_check_type]`

* **type**: The type of validation to apply.
* **value**: The value to search for in the previous query result.


### Next Steps
--------------
Depending on your use case, you may need to:

* Integrate this validation config into an application or workflow.
* Modify the query or column selection to suit your specific requirements.
* Add additional validations to the `validations` block.
