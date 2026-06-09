use std::collections::BTreeMap;
use std::fs;
use std::path::{Component, Path, PathBuf};

use calamine::{open_workbook_auto, Data, Reader};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::error::{ApiError, ApiResult};

const DEFAULT_SAMPLE_ROWS: usize = 20;
const MAX_SAMPLE_ROWS: usize = 200;
#[cfg(not(test))]
const MAX_INSPECT_ROWS: usize = 100_000;
#[cfg(test)]
const MAX_INSPECT_ROWS: usize = 100;
const MAX_WRITE_ROWS: usize = 1_000_000;
const MAX_WRITE_COLUMNS: usize = 16_384;

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub enum DataFileFormat {
    Csv,
    Xlsx,
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct DataInspectRequest {
    pub source_path: String,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional = nullable))]
    pub format: Option<DataFileFormat>,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional = nullable))]
    pub sample_rows: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct DataWriteRequest {
    pub target_path: String,
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional = nullable))]
    pub format: Option<DataFileFormat>,
    #[serde(default)]
    pub overwrite: bool,
    pub sheets: Vec<DataSheetInput>,
}

#[derive(Debug, Deserialize, Serialize)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct DataSheetInput {
    #[serde(default)]
    #[cfg_attr(feature = "ts-export", ts(optional = nullable))]
    pub name: Option<String>,
    pub rows: Vec<BTreeMap<String, Value>>,
}

#[derive(Debug, Serialize, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct DataInspectResponse {
    pub path: String,
    pub format: DataFileFormat,
    pub sheets: Vec<DataSheetSummary>,
}

#[derive(Debug, Serialize, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct DataSheetSummary {
    pub name: String,
    pub rows: usize,
    pub truncated: bool,
    pub warnings: Vec<String>,
    pub columns: Vec<DataColumnSummary>,
    pub sample_rows: Vec<BTreeMap<String, Value>>,
}

#[derive(Debug, Serialize, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct DataColumnSummary {
    pub name: String,
    pub inferred_type: DataColumnType,
    pub nulls: usize,
    pub non_nulls: usize,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub enum DataColumnType {
    Empty,
    Boolean,
    Integer,
    Number,
    String,
    Mixed,
}

#[derive(Debug, Serialize, PartialEq)]
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(feature = "ts-export", ts(export, export_to = "../../../bindings/"))]
pub struct DataWriteResponse {
    pub path: String,
    pub format: DataFileFormat,
    pub sheets_written: usize,
    pub rows_written: usize,
}

pub fn inspect_data_file(req: DataInspectRequest) -> ApiResult<DataInspectResponse> {
    let path = resolve_read_path(req.source_path.trim())?;
    validate_existing_file(&path)?;
    let format = req
        .format
        .unwrap_or_else(|| infer_format(&path).unwrap_or(DataFileFormat::Csv));
    let sample_limit = req
        .sample_rows
        .unwrap_or(DEFAULT_SAMPLE_ROWS)
        .min(MAX_SAMPLE_ROWS);

    let sheets = match format {
        DataFileFormat::Csv => vec![inspect_csv(&path, sample_limit)?],
        DataFileFormat::Xlsx => inspect_xlsx(&path, sample_limit)?,
    };

    Ok(DataInspectResponse {
        path: path.display().to_string(),
        format,
        sheets,
    })
}

pub fn write_data_file(req: DataWriteRequest) -> ApiResult<DataWriteResponse> {
    let path = resolve_write_path(req.target_path.trim())?;
    if path.exists() && !req.overwrite {
        return Err(ApiError::BadRequest(format!(
            "target_path already exists: {}",
            path.display()
        )));
    }
    if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
        fs::create_dir_all(parent)
            .map_err(|e| ApiError::Internal(format!("creating target directory failed: {e}")))?;
    }

    let format = req
        .format
        .unwrap_or_else(|| infer_format(&path).unwrap_or(DataFileFormat::Csv));
    validate_write_request(&req, format)?;
    let rows_written = req.sheets.iter().map(|sheet| sheet.rows.len()).sum();
    match format {
        DataFileFormat::Csv => write_csv(&path, &req.sheets[0])?,
        DataFileFormat::Xlsx => write_xlsx(&path, &req.sheets)?,
    }

    Ok(DataWriteResponse {
        path: path.display().to_string(),
        format,
        sheets_written: req.sheets.len(),
        rows_written,
    })
}

fn resolve_read_path(raw: &str) -> ApiResult<PathBuf> {
    if raw.is_empty() {
        return Err(ApiError::BadRequest("source_path is required".into()));
    }
    let root = data_root()?;
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        reject_parent_dir(&path, "source_path")?;
    }
    let resolved = if path.is_absolute() {
        path.clone()
    } else {
        root.join(&path)
    };
    let canonical = resolved
        .canonicalize()
        .map_err(|_| ApiError::NotFound(format!("source_path not found: {}", path.display())))?;
    ensure_under_root(&canonical, &root, "source_path")?;
    Ok(canonical)
}

fn resolve_write_path(raw: &str) -> ApiResult<PathBuf> {
    if raw.is_empty() {
        return Err(ApiError::BadRequest("target_path is required".into()));
    }
    let root = data_root()?;
    let path = PathBuf::from(raw);
    if !path.is_absolute() {
        reject_parent_dir(&path, "target_path")?;
    }
    let resolved = if path.is_absolute() {
        path.clone()
    } else {
        root.join(&path)
    };
    if fs::symlink_metadata(&resolved)
        .map(|metadata| metadata.file_type().is_symlink())
        .unwrap_or(false)
    {
        let canonical = resolved.canonicalize().map_err(|e| {
            ApiError::BadRequest(format!("target_path symlink cannot be resolved: {e}"))
        })?;
        ensure_under_root(&canonical, &root, "target_path")?;
        return Ok(canonical);
    }
    if resolved.exists() {
        let canonical = resolved
            .canonicalize()
            .map_err(|e| ApiError::BadRequest(format!("target_path cannot be resolved: {e}")))?;
        ensure_under_root(&canonical, &root, "target_path")?;
        return Ok(canonical);
    }
    let existing_parent = existing_parent(resolved.parent().unwrap_or(&root), &root)?;
    let canonical_parent = existing_parent
        .canonicalize()
        .map_err(|e| ApiError::BadRequest(format!("target_path parent cannot be resolved: {e}")))?;
    ensure_under_root(&canonical_parent, &root, "target_path")?;
    Ok(resolved)
}

fn data_root() -> ApiResult<PathBuf> {
    let root = match std::env::var("HARNESS_DATA_ROOT") {
        Ok(raw) if !raw.trim().is_empty() => PathBuf::from(raw.trim()),
        _ => std::env::current_dir()
            .map_err(|e| ApiError::Internal(format!("resolving current dir failed: {e}")))?,
    };
    let canonical = root
        .canonicalize()
        .map_err(|e| ApiError::Internal(format!("resolving data root failed: {e}")))?;
    if !canonical.is_dir() {
        return Err(ApiError::Internal(format!(
            "data root is not a directory: {}",
            canonical.display()
        )));
    }
    Ok(canonical)
}

fn existing_parent<'a>(path: &'a Path, root: &'a Path) -> ApiResult<&'a Path> {
    let mut current = path;
    loop {
        if current.exists() {
            return Ok(current);
        }
        current = current.parent().ok_or_else(|| {
            ApiError::BadRequest(format!(
                "target_path parent escapes data root: {}",
                root.display()
            ))
        })?;
    }
}

fn reject_parent_dir(path: &Path, field: &str) -> ApiResult<()> {
    if path
        .components()
        .any(|component| matches!(component, Component::ParentDir))
    {
        return Err(ApiError::BadRequest(format!(
            "{field} must not contain parent directory traversal"
        )));
    }
    Ok(())
}

fn ensure_under_root(path: &Path, root: &Path, field: &str) -> ApiResult<()> {
    if !path.starts_with(root) {
        return Err(ApiError::BadRequest(format!(
            "{field} escapes the configured data root"
        )));
    }
    Ok(())
}

fn validate_existing_file(path: &Path) -> ApiResult<()> {
    if path.as_os_str().is_empty() {
        return Err(ApiError::BadRequest("source_path is required".into()));
    }
    if !path.exists() {
        return Err(ApiError::NotFound(format!(
            "source_path not found: {}",
            path.display()
        )));
    }
    if !path.is_file() {
        return Err(ApiError::BadRequest(format!(
            "source_path is not a file: {}",
            path.display()
        )));
    }
    Ok(())
}

fn validate_write_request(req: &DataWriteRequest, format: DataFileFormat) -> ApiResult<()> {
    if req.sheets.is_empty() {
        return Err(ApiError::BadRequest(
            "at least one sheet is required".into(),
        ));
    }
    if format == DataFileFormat::Csv && req.sheets.len() != 1 {
        return Err(ApiError::BadRequest(
            "CSV output accepts exactly one sheet".into(),
        ));
    }
    for sheet in &req.sheets {
        if sheet.rows.len() > MAX_WRITE_ROWS {
            return Err(ApiError::BadRequest(format!(
                "sheet exceeds max rows: {}",
                MAX_WRITE_ROWS
            )));
        }
        let columns = ordered_columns(sheet);
        if columns.len() > MAX_WRITE_COLUMNS {
            return Err(ApiError::BadRequest(format!(
                "sheet exceeds max columns: {}",
                MAX_WRITE_COLUMNS
            )));
        }
    }
    Ok(())
}

fn infer_format(path: &Path) -> Option<DataFileFormat> {
    match path
        .extension()
        .and_then(|ext| ext.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "csv" | "tsv" => Some(DataFileFormat::Csv),
        "xlsx" | "xlsm" | "xlsb" | "xls" => Some(DataFileFormat::Xlsx),
        _ => None,
    }
}

fn inspect_csv(path: &Path, sample_limit: usize) -> ApiResult<DataSheetSummary> {
    let mut reader = csv::Reader::from_path(path)
        .map_err(|e| ApiError::BadRequest(format!("reading CSV failed: {e}")))?;
    let headers = reader
        .headers()
        .map_err(|e| ApiError::BadRequest(format!("reading CSV headers failed: {e}")))?
        .iter()
        .map(normalize_header)
        .collect::<Vec<_>>();
    let warnings = duplicate_header_warnings(&headers);
    let mut rows = Vec::new();
    let mut truncated = false;
    for record in reader.records() {
        if rows.len() >= MAX_INSPECT_ROWS {
            truncated = true;
            break;
        }
        let record =
            record.map_err(|e| ApiError::BadRequest(format!("reading CSV row failed: {e}")))?;
        let row = headers
            .iter()
            .enumerate()
            .map(|(idx, header)| {
                let value = record.get(idx).unwrap_or_default();
                (header.clone(), csv_value(value))
            })
            .collect();
        rows.push(row);
    }
    Ok(summarize_sheet(
        "Sheet1".into(),
        rows,
        sample_limit,
        truncated,
        warnings,
    ))
}

fn inspect_xlsx(path: &Path, sample_limit: usize) -> ApiResult<Vec<DataSheetSummary>> {
    let mut workbook = open_workbook_auto(path)
        .map_err(|e| ApiError::BadRequest(format!("reading workbook failed: {e}")))?;
    let sheet_names = workbook.sheet_names().to_vec();
    let mut summaries = Vec::new();
    for sheet_name in sheet_names {
        let range = workbook
            .worksheet_range(&sheet_name)
            .map_err(|e| ApiError::BadRequest(format!("reading sheet {sheet_name} failed: {e}")))?;
        let mut iter = range.rows();
        let Some(header_row) = iter.next() else {
            summaries.push(DataSheetSummary {
                name: sheet_name,
                rows: 0,
                truncated: false,
                warnings: Vec::new(),
                columns: Vec::new(),
                sample_rows: Vec::new(),
            });
            continue;
        };
        let headers = header_row
            .iter()
            .enumerate()
            .map(|(idx, cell)| normalize_header_with_fallback(&xlsx_cell_to_value(cell), idx))
            .collect::<Vec<_>>();
        let mut rows = Vec::new();
        let mut truncated = false;
        for row in iter {
            if rows.len() >= MAX_INSPECT_ROWS {
                truncated = true;
                break;
            }
            rows.push(
                headers
                    .iter()
                    .enumerate()
                    .map(|(idx, header)| {
                        let value = row.get(idx).map(xlsx_cell_to_value).unwrap_or(Value::Null);
                        (header.clone(), value)
                    })
                    .collect::<BTreeMap<_, _>>(),
            );
        }
        summaries.push(summarize_sheet(
            sheet_name,
            rows,
            sample_limit,
            truncated,
            Vec::new(),
        ));
    }
    Ok(summaries)
}

fn summarize_sheet(
    name: String,
    rows: Vec<BTreeMap<String, Value>>,
    sample_limit: usize,
    truncated: bool,
    warnings: Vec<String>,
) -> DataSheetSummary {
    let columns = rows.iter().flat_map(|row| row.keys().cloned()).fold(
        Vec::<String>::new(),
        |mut acc, col| {
            if !acc.contains(&col) {
                acc.push(col);
            }
            acc
        },
    );
    let summaries = columns
        .iter()
        .map(|name| summarize_column(name, &rows))
        .collect();
    let sample_rows = rows.iter().take(sample_limit).cloned().collect();
    DataSheetSummary {
        name,
        rows: rows.len(),
        truncated,
        warnings,
        columns: summaries,
        sample_rows,
    }
}

fn duplicate_header_warnings(headers: &[String]) -> Vec<String> {
    let mut counts = BTreeMap::<&str, usize>::new();
    for header in headers {
        *counts.entry(header.as_str()).or_default() += 1;
    }
    counts
        .into_iter()
        .filter_map(|(header, count)| {
            (count > 1).then(|| {
                format!(
                    "CSV header `{header}` appears {count} times; duplicate columns are collapsed in row objects"
                )
            })
        })
        .collect()
}

fn summarize_column(name: &str, rows: &[BTreeMap<String, Value>]) -> DataColumnSummary {
    let mut nulls = 0;
    let mut non_nulls = 0;
    let mut inferred: Option<DataColumnType> = None;
    for row in rows {
        let value = row.get(name).unwrap_or(&Value::Null);
        if is_nullish(value) {
            nulls += 1;
            continue;
        }
        non_nulls += 1;
        let current = value_type(value);
        inferred = Some(match inferred {
            None => current,
            Some(prev) => merge_types(prev, current),
        });
    }
    DataColumnSummary {
        name: name.to_string(),
        inferred_type: inferred.unwrap_or(DataColumnType::Empty),
        nulls,
        non_nulls,
    }
}

fn merge_types(left: DataColumnType, right: DataColumnType) -> DataColumnType {
    match (left, right) {
        (a, b) if a == b => a,
        (DataColumnType::Integer, DataColumnType::Number)
        | (DataColumnType::Number, DataColumnType::Integer) => DataColumnType::Number,
        _ => DataColumnType::Mixed,
    }
}

fn value_type(value: &Value) -> DataColumnType {
    match value {
        Value::Bool(_) => DataColumnType::Boolean,
        Value::Number(n) if n.is_i64() || n.is_u64() => DataColumnType::Integer,
        Value::Number(_) => DataColumnType::Number,
        Value::String(s) => parse_string_type(s),
        _ => DataColumnType::String,
    }
}

fn parse_string_type(value: &str) -> DataColumnType {
    let trimmed = value.trim();
    if trimmed.eq_ignore_ascii_case("true") || trimmed.eq_ignore_ascii_case("false") {
        DataColumnType::Boolean
    } else if trimmed.parse::<i64>().is_ok() {
        DataColumnType::Integer
    } else if trimmed.parse::<f64>().is_ok() {
        DataColumnType::Number
    } else {
        DataColumnType::String
    }
}

fn csv_value(raw: &str) -> Value {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        Value::Null
    } else if trimmed.eq_ignore_ascii_case("true") {
        Value::Bool(true)
    } else if trimmed.eq_ignore_ascii_case("false") {
        Value::Bool(false)
    } else if let Ok(value) = trimmed.parse::<i64>() {
        Value::Number(value.into())
    } else if let Ok(value) = trimmed.parse::<f64>() {
        serde_json::Number::from_f64(value)
            .map(Value::Number)
            .unwrap_or_else(|| Value::String(raw.to_string()))
    } else {
        Value::String(raw.to_string())
    }
}

fn xlsx_cell_to_value(cell: &Data) -> Value {
    match cell {
        Data::Empty => Value::Null,
        Data::String(s) | Data::DateTimeIso(s) | Data::DurationIso(s) => Value::String(s.clone()),
        Data::Float(n) => serde_json::Number::from_f64(*n)
            .map(Value::Number)
            .unwrap_or(Value::Null),
        Data::Int(n) => Value::Number((*n).into()),
        Data::Bool(b) => Value::Bool(*b),
        Data::Error(e) => Value::String(format!("{e:?}")),
        Data::DateTime(dt) => Value::String(dt.to_string()),
    }
}

fn normalize_header(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        "column".into()
    } else {
        trimmed.to_string()
    }
}

fn normalize_header_with_fallback(value: &Value, idx: usize) -> String {
    match value {
        Value::String(s) if !s.trim().is_empty() => s.trim().to_string(),
        Value::Number(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        _ => format!("column_{}", idx + 1),
    }
}

fn is_nullish(value: &Value) -> bool {
    matches!(value, Value::Null) || matches!(value, Value::String(s) if s.trim().is_empty())
}

fn ordered_columns(sheet: &DataSheetInput) -> Vec<String> {
    sheet.rows.iter().flat_map(|row| row.keys().cloned()).fold(
        Vec::<String>::new(),
        |mut acc, col| {
            if !acc.contains(&col) {
                acc.push(col);
            }
            acc
        },
    )
}

fn write_csv(path: &Path, sheet: &DataSheetInput) -> ApiResult<()> {
    let columns = ordered_columns(sheet);
    let mut writer = csv::Writer::from_path(path)
        .map_err(|e| ApiError::Internal(format!("creating CSV failed: {e}")))?;
    writer
        .write_record(&columns)
        .map_err(|e| ApiError::Internal(format!("writing CSV headers failed: {e}")))?;
    for row in &sheet.rows {
        let record = columns
            .iter()
            .map(|column| cell_to_string(row.get(column).unwrap_or(&Value::Null)))
            .collect::<Vec<_>>();
        writer
            .write_record(record)
            .map_err(|e| ApiError::Internal(format!("writing CSV row failed: {e}")))?;
    }
    writer
        .flush()
        .map_err(|e| ApiError::Internal(format!("flushing CSV failed: {e}")))?;
    Ok(())
}

fn write_xlsx(path: &Path, sheets: &[DataSheetInput]) -> ApiResult<()> {
    let mut workbook = rust_xlsxwriter::Workbook::new();
    for (sheet_idx, sheet) in sheets.iter().enumerate() {
        let worksheet = workbook.add_worksheet();
        let name = sheet
            .name
            .as_deref()
            .filter(|name| !name.trim().is_empty())
            .map(str::trim)
            .map(str::to_string)
            .unwrap_or_else(|| format!("Sheet{}", sheet_idx + 1));
        worksheet
            .set_name(&name)
            .map_err(|e| ApiError::BadRequest(format!("invalid sheet name {name:?}: {e}")))?;
        let columns = ordered_columns(sheet);
        for (col_idx, column) in columns.iter().enumerate() {
            worksheet
                .write_string(0, col_idx as u16, column)
                .map_err(|e| ApiError::Internal(format!("writing XLSX header failed: {e}")))?;
        }
        for (row_idx, row) in sheet.rows.iter().enumerate() {
            let xlsx_row = (row_idx + 1) as u32;
            for (col_idx, column) in columns.iter().enumerate() {
                write_xlsx_cell(worksheet, xlsx_row, col_idx as u16, row.get(column))?;
            }
        }
    }
    workbook
        .save(path)
        .map_err(|e| ApiError::Internal(format!("saving XLSX failed: {e}")))?;
    Ok(())
}

fn write_xlsx_cell(
    worksheet: &mut rust_xlsxwriter::Worksheet,
    row: u32,
    col: u16,
    value: Option<&Value>,
) -> ApiResult<()> {
    match value.unwrap_or(&Value::Null) {
        Value::Null => Ok(()),
        Value::Bool(v) => worksheet
            .write_boolean(row, col, *v)
            .map(|_| ())
            .map_err(|e| ApiError::Internal(format!("writing XLSX bool failed: {e}"))),
        Value::Number(n) => {
            if let Some(v) = n.as_f64() {
                worksheet
                    .write_number(row, col, v)
                    .map(|_| ())
                    .map_err(|e| ApiError::Internal(format!("writing XLSX number failed: {e}")))
            } else {
                worksheet
                    .write_string(row, col, n.to_string())
                    .map(|_| ())
                    .map_err(|e| ApiError::Internal(format!("writing XLSX string failed: {e}")))
            }
        }
        Value::String(v) => worksheet
            .write_string(row, col, v)
            .map(|_| ())
            .map_err(|e| ApiError::Internal(format!("writing XLSX string failed: {e}"))),
        other => worksheet
            .write_string(row, col, other.to_string())
            .map(|_| ())
            .map_err(|e| ApiError::Internal(format!("writing XLSX string failed: {e}"))),
    }
}

fn cell_to_string(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::{Mutex, OnceLock};
    use tempfile::tempdir;

    fn with_data_root<T>(root: &Path, f: impl FnOnce() -> T) -> T {
        static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        let _guard = ENV_LOCK
            .get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let previous = std::env::var_os("HARNESS_DATA_ROOT");
        std::env::set_var("HARNESS_DATA_ROOT", root);
        let result = f();
        if let Some(previous) = previous {
            std::env::set_var("HARNESS_DATA_ROOT", previous);
        } else {
            std::env::remove_var("HARNESS_DATA_ROOT");
        }
        result
    }

    #[test]
    fn inspects_csv_with_types_and_samples() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("sales.csv");
        fs::write(
            &path,
            "name,total,paid\nAda,10,true\nGrace,12.5,false\nLinus,,true\n",
        )
        .unwrap();

        with_data_root(dir.path(), || {
            let result = inspect_data_file(DataInspectRequest {
                source_path: path.display().to_string(),
                format: None,
                sample_rows: Some(2),
            })
            .unwrap();

            assert_eq!(result.format, DataFileFormat::Csv);
            assert_eq!(result.sheets[0].rows, 3);
            assert!(!result.sheets[0].truncated);
            assert_eq!(result.sheets[0].sample_rows.len(), 2);
            let total = result.sheets[0]
                .columns
                .iter()
                .find(|column| column.name == "total")
                .unwrap();
            assert_eq!(total.inferred_type, DataColumnType::Number);
            assert_eq!(total.nulls, 1);
        });
    }

    #[test]
    fn writes_csv_from_json_rows() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("out.csv");
        let mut row = BTreeMap::new();
        row.insert("name".into(), json!("Ada"));
        row.insert("total".into(), json!(10));

        with_data_root(dir.path(), || {
            let response = write_data_file(DataWriteRequest {
                target_path: path.display().to_string(),
                format: None,
                overwrite: false,
                sheets: vec![DataSheetInput {
                    name: None,
                    rows: vec![row],
                }],
            })
            .unwrap();

            assert_eq!(response.rows_written, 1);
            let text = fs::read_to_string(path).unwrap();
            assert!(text.contains("name,total"));
            assert!(text.contains("Ada,10"));
        });
    }

    #[test]
    fn writes_and_inspects_csv_roundtrip() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("roundtrip.csv");
        let mut first = BTreeMap::new();
        first.insert("name".into(), json!("Ada"));
        first.insert("active".into(), json!(true));
        first.insert("count".into(), json!(10));
        let mut second = BTreeMap::new();
        second.insert("name".into(), json!("Grace"));
        second.insert("active".into(), json!(false));
        second.insert("count".into(), json!(12));

        with_data_root(dir.path(), || {
            write_data_file(DataWriteRequest {
                target_path: path.display().to_string(),
                format: Some(DataFileFormat::Csv),
                overwrite: false,
                sheets: vec![DataSheetInput {
                    name: None,
                    rows: vec![first, second],
                }],
            })
            .unwrap();

            let inspected = inspect_data_file(DataInspectRequest {
                source_path: path.display().to_string(),
                format: Some(DataFileFormat::Csv),
                sample_rows: Some(10),
            })
            .unwrap();

            assert_eq!(inspected.sheets[0].rows, 2);
            assert!(!inspected.sheets[0].truncated);
            assert_eq!(inspected.sheets[0].sample_rows[0]["name"], json!("Ada"));
            let active = inspected.sheets[0]
                .columns
                .iter()
                .find(|column| column.name == "active")
                .unwrap();
            assert_eq!(active.inferred_type, DataColumnType::Boolean);
            let count = inspected.sheets[0]
                .columns
                .iter()
                .find(|column| column.name == "count")
                .unwrap();
            assert_eq!(count.inferred_type, DataColumnType::Integer);
        });
    }

    #[test]
    fn inspect_caps_sample_rows() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("many.csv");
        let mut csv = String::from("id\n");
        for idx in 0..(MAX_INSPECT_ROWS + 50) {
            csv.push_str(&format!("{idx}\n"));
        }
        fs::write(&path, csv).unwrap();

        with_data_root(dir.path(), || {
            let inspected = inspect_data_file(DataInspectRequest {
                source_path: path.display().to_string(),
                format: Some(DataFileFormat::Csv),
                sample_rows: Some(MAX_SAMPLE_ROWS + 50),
            })
            .unwrap();

            assert_eq!(inspected.sheets[0].rows, MAX_INSPECT_ROWS);
            assert!(inspected.sheets[0].truncated);
            assert_eq!(
                inspected.sheets[0].sample_rows.len(),
                MAX_SAMPLE_ROWS.min(MAX_INSPECT_ROWS)
            );
        });
    }

    #[test]
    fn inspect_reports_duplicate_csv_headers() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("duplicates.csv");
        fs::write(&path, "name,name\nAda,Lovelace\n").unwrap();

        with_data_root(dir.path(), || {
            let inspected = inspect_data_file(DataInspectRequest {
                source_path: path.display().to_string(),
                format: Some(DataFileFormat::Csv),
                sample_rows: None,
            })
            .unwrap();

            assert!(inspected.sheets[0]
                .warnings
                .iter()
                .any(|warning| warning.contains("duplicate columns are collapsed")));
        });
    }

    #[test]
    fn write_validation_rejects_row_and_column_limits() {
        let too_many_rows = DataWriteRequest {
            target_path: "too-many-rows.csv".into(),
            format: Some(DataFileFormat::Csv),
            overwrite: false,
            sheets: vec![DataSheetInput {
                name: None,
                rows: vec![BTreeMap::new(); MAX_WRITE_ROWS + 1],
            }],
        };
        assert!(matches!(
            validate_write_request(&too_many_rows, DataFileFormat::Csv),
            Err(ApiError::BadRequest(message)) if message.contains("max rows")
        ));

        let row = (0..=MAX_WRITE_COLUMNS)
            .map(|idx| (format!("col_{idx}"), Value::Null))
            .collect::<BTreeMap<_, _>>();
        let too_many_columns = DataWriteRequest {
            target_path: "too-many-columns.csv".into(),
            format: Some(DataFileFormat::Csv),
            overwrite: false,
            sheets: vec![DataSheetInput {
                name: None,
                rows: vec![row],
            }],
        };
        assert!(matches!(
            validate_write_request(&too_many_columns, DataFileFormat::Csv),
            Err(ApiError::BadRequest(message)) if message.contains("max columns")
        ));
    }

    #[test]
    fn relative_paths_must_not_escape_current_dir() {
        let dir = tempdir().unwrap();
        with_data_root(dir.path(), || {
            assert!(matches!(
                inspect_data_file(DataInspectRequest {
                    source_path: "../outside.csv".into(),
                    format: Some(DataFileFormat::Csv),
                    sample_rows: None,
                }),
                Err(ApiError::BadRequest(message)) if message.contains("traversal")
            ));

            assert!(matches!(
                write_data_file(DataWriteRequest {
                    target_path: "../outside.csv".into(),
                    format: Some(DataFileFormat::Csv),
                    overwrite: true,
                    sheets: vec![DataSheetInput {
                        name: None,
                        rows: Vec::new(),
                    }],
                }),
                Err(ApiError::BadRequest(message)) if message.contains("traversal")
            ));
        });
    }

    #[test]
    fn absolute_paths_are_confined_to_data_root() {
        let root = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let inside_path = root.path().join("inside.csv");
        let outside_read_path = outside.path().join("outside.csv");
        let outside_write_path = outside.path().join("write.csv");
        fs::write(&inside_path, "name\nAda\n").unwrap();
        fs::write(&outside_read_path, "name\nGrace\n").unwrap();

        with_data_root(root.path(), || {
            let inspected = inspect_data_file(DataInspectRequest {
                source_path: inside_path.display().to_string(),
                format: Some(DataFileFormat::Csv),
                sample_rows: None,
            })
            .unwrap();
            assert_eq!(inspected.sheets[0].rows, 1);

            assert!(matches!(
                inspect_data_file(DataInspectRequest {
                    source_path: outside_read_path.display().to_string(),
                    format: Some(DataFileFormat::Csv),
                    sample_rows: None,
                }),
                Err(ApiError::BadRequest(message)) if message.contains("data root")
            ));

            assert!(matches!(
                write_data_file(DataWriteRequest {
                    target_path: outside_write_path.display().to_string(),
                    format: Some(DataFileFormat::Csv),
                    overwrite: true,
                    sheets: vec![DataSheetInput {
                        name: None,
                        rows: Vec::new(),
                    }],
                }),
                Err(ApiError::BadRequest(message)) if message.contains("data root")
            ));
        });
    }

    #[cfg(unix)]
    #[test]
    fn symlink_that_escapes_data_root_is_rejected() {
        let root = tempdir().unwrap();
        let outside = tempdir().unwrap();
        let outside_path = outside.path().join("outside.csv");
        let link_path = root.path().join("link.csv");
        fs::write(&outside_path, "name\nAda\n").unwrap();
        std::os::unix::fs::symlink(&outside_path, &link_path).unwrap();

        with_data_root(root.path(), || {
            assert!(matches!(
                inspect_data_file(DataInspectRequest {
                    source_path: link_path.display().to_string(),
                    format: Some(DataFileFormat::Csv),
                    sample_rows: None,
                }),
                Err(ApiError::BadRequest(message)) if message.contains("data root")
            ));
        });
    }

    #[test]
    fn inspect_reports_format_errors() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("not-a-workbook.csv");
        fs::write(&path, "name,total\nAda,10\n").unwrap();

        with_data_root(dir.path(), || {
            assert!(matches!(
                inspect_data_file(DataInspectRequest {
                    source_path: path.display().to_string(),
                    format: Some(DataFileFormat::Xlsx),
                    sample_rows: None,
                }),
                Err(ApiError::BadRequest(message)) if message.contains("workbook")
            ));
        });
    }

    #[test]
    fn writes_and_inspects_xlsx() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("out.xlsx");
        let mut row = BTreeMap::new();
        row.insert("name".into(), json!("Ada"));
        row.insert("total".into(), json!(10.5));

        with_data_root(dir.path(), || {
            write_data_file(DataWriteRequest {
                target_path: path.display().to_string(),
                format: None,
                overwrite: false,
                sheets: vec![DataSheetInput {
                    name: Some("Sales".into()),
                    rows: vec![row],
                }],
            })
            .unwrap();

            let result = inspect_data_file(DataInspectRequest {
                source_path: path.display().to_string(),
                format: None,
                sample_rows: Some(10),
            })
            .unwrap();
            assert_eq!(result.format, DataFileFormat::Xlsx);
            assert_eq!(result.sheets[0].name, "Sales");
            assert_eq!(result.sheets[0].rows, 1);
            assert!(!result.sheets[0].truncated);
            let total = result.sheets[0]
                .columns
                .iter()
                .find(|column| column.name == "total")
                .unwrap();
            assert_eq!(total.inferred_type, DataColumnType::Number);
        });
    }
}
