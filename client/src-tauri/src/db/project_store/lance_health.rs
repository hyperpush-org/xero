use arrow_schema::Schema;

use crate::commands::CommandError;

pub(crate) fn table_schema_supports_expected(
    table_schema: &Schema,
    expected_schema: &Schema,
) -> bool {
    expected_schema.fields().iter().all(|expected| {
        let Ok(actual) = table_schema.field_with_name(expected.name()) else {
            return false;
        };
        actual.data_type() == expected.data_type() && actual.is_nullable() == expected.is_nullable()
    })
}

pub(crate) fn schema_drift_error(
    code: &'static str,
    store_label: &str,
    column: &str,
) -> CommandError {
    CommandError::system_fault(
        code,
        format!("Xero {store_label} Lance dataset is missing expected column `{column}`."),
    )
}
