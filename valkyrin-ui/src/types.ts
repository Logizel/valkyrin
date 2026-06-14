/**
 * Valkyrin TypeScript Type Definitions
 * Mirrors the Rust IR and Canvas JSON schema exactly
 */

/**
 * All supported data types in Valkyrin
 * Must match the Rust DataType enum in valkyrin-core/src/ir.rs
 */
export type DataType = 
  | "string" 
  | "text" 
  | "smallint" 
  | "int" 
  | "bigint" 
  | "float" 
  | "decimal" 
  | "boolean" 
  | "datetime" 
  | "json" 
  | "uuid" 
  | "enum";

/**
 * Relation types supported in the visual canvas
 */
export type RelationType = "1:1" | "1:N" | "M:N";

/**
 * A single column/field definition
 * Maps to Rust Field + Constraints
 */
export interface CanvasColumn {
  id: string;
  name: string;
  raw_type: DataType;
  is_primary: boolean;
  is_nullable: boolean;
  is_unique: boolean;
  is_indexed: boolean;
  default_value?: string;
  enum_values?: string[]; // For enum type
  precision?: number; // For decimal type
  scale?: number; // For decimal type
  max_length?: number; // For string type (optional, unlimited if undefined)
}

/**
 * A table/entity definition
 * Maps to Rust Entity
 */
export interface CanvasTable {
  id: string;
  name: string;
  columns: CanvasColumn[];
  position: NodePosition;
}

/**
 * A relationship/edge between two tables
 */
export interface CanvasRelation {
  id: string;
  source_table_id: string;
  target_table_id: string;
  relation_type: RelationType;
}

/**
 * 2D position on the canvas
 */
export interface NodePosition {
  x: number;
  y: number;
}

/**
 * The complete canvas blueprint, persisted to schema.vdb.json
 */
export interface CanvasPayload {
  tables: CanvasTable[];
  relations: CanvasRelation[];
}

/**
 * React Flow node data shape for table nodes
 * Must extend Record<string, unknown> for React Flow compatibility
 */
export interface TableNodeData extends Record<string, unknown> {
  label: string; // Table name
  columns: CanvasColumn[];
  onAddColumn?: (nodeId: string) => void;
  onDeleteColumn?: (nodeId: string, colId: string) => void;
  onDeleteTable?: (nodeId: string) => void;
  onEditTable?: (nodeId: string, newName: string) => void;
  onEditColumn?: (nodeId: string, colId: string, newColumn: CanvasColumn) => void;
  onOpenProperties?: (nodeId: string, colId?: string) => void;
}

/**
 * Validation result type
 */
export interface ValidationError {
  field: string;
  message: string;
}

/**
 * Validation result with status
 */
export interface ValidationResult {
  valid: boolean;
  errors: ValidationError[];
}

/**
 * Validation utilities for identifiers and constraints
 */

/**
 * Check if an identifier (table or column name) is valid
 * Must match /^[a-zA-Z_][a-zA-Z0-9_]*$/
 */
export function isValidIdentifier(name: string): boolean {
  const identifierRegex = /^[a-zA-Z_][a-zA-Z0-9_]*$/;
  return identifierRegex.test(name) && name.length > 0;
}

/**
 * Validate a table name
 */
export function validateTableName(
  name: string,
  existingTableNames: string[],
  excludeId?: string // When editing, exclude the current table
): ValidationResult {
  const errors: ValidationError[] = [];

  if (!name.trim()) {
    errors.push({ field: "name", message: "Table name is required" });
  } else if (!isValidIdentifier(name)) {
    errors.push({
      field: "name",
      message: "Table name must start with letter or underscore, followed by alphanumerics or underscores only",
    });
  }

  // Check for duplicates
  const isDuplicate = existingTableNames.some(
    (existing) => existing === name && existing !== excludeId
  );
  if (isDuplicate) {
    errors.push({
      field: "name",
      message: "A table with this name already exists",
    });
  }

  return { valid: errors.length === 0, errors };
}

/**
 * Validate a column name within a table
 */
export function validateColumnName(
  name: string,
  existingColumnNames: string[],
  excludeId?: string // When editing, exclude the current column
): ValidationResult {
  const errors: ValidationError[] = [];

  if (!name.trim()) {
    errors.push({ field: "name", message: "Column name is required" });
  } else if (!isValidIdentifier(name)) {
    errors.push({
      field: "name",
      message: "Column name must start with letter or underscore, followed by alphanumerics or underscores only",
    });
  }

  // Check for duplicates within the table
  const isDuplicate = existingColumnNames.some(
    (existing) => existing === name && existing !== excludeId
  );
  if (isDuplicate) {
    errors.push({
      field: "name",
      message: "A column with this name already exists in this table",
    });
  }

  return { valid: errors.length === 0, errors };
}

/**
 * Validate a decimal precision and scale
 */
export function validateDecimal(
  precision: number | undefined,
  scale: number | undefined
): ValidationResult {
  const errors: ValidationError[] = [];

  if (precision === undefined || precision < 1 || precision > 65) {
    errors.push({
      field: "precision",
      message: "Precision must be between 1 and 65",
    });
  }

  if (scale === undefined || scale < 0 || scale > (precision || 65)) {
    errors.push({
      field: "scale",
      message: "Scale must be between 0 and precision",
    });
  }

  return { valid: errors.length === 0, errors };
}

/**
 * Validate enum values (comma-separated)
 */
export function validateEnumValues(input: string): ValidationResult {
  const errors: ValidationError[] = [];

  if (!input.trim()) {
    errors.push({
      field: "enum_values",
      message: "Enum must have at least one value",
    });
    return { valid: false, errors };
  }

  const values = input.split(",").map((v) => v.trim());

  if (values.some((v) => !isValidIdentifier(v))) {
    errors.push({
      field: "enum_values",
      message: "Each enum value must be a valid identifier",
    });
  }

  if (new Set(values).size !== values.length) {
    errors.push({
      field: "enum_values",
      message: "Enum values must be unique",
    });
  }

  return { valid: errors.length === 0, errors };
}

// Create a namespace-like object for backward compatibility
export const Validation = {
  isValidIdentifier,
  validateTableName,
  validateColumnName,
  validateDecimal,
  validateEnumValues,
};
