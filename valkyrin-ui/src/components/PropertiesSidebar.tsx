import { useState, useEffect } from "react";
import type {
  CanvasColumn,
  CanvasTable,
  DataType,
  ValidationError,
} from "../types";
import { Validation } from "../types";

interface PropertiesSidebarProps {
  isOpen: boolean;
  onClose: () => void;
  tableId?: string;
  table?: CanvasTable;
  column?: CanvasColumn;
  existingTableNames: string[];
  existingColumnNames: string[];
  onSaveTable?: (tableId: string, newName: string) => void;
  onSaveColumn?: (
    tableId: string,
    columnId: string,
    column: CanvasColumn
  ) => void;
}

/**
 * Properties Sidebar: Beautiful right-hand panel for editing tables and columns
 * Replaces all window.prompt() inputs with a sleek, validated form experience
 */
export default function PropertiesSidebar({
  isOpen,
  onClose,
  tableId,
  table,
  column,
  existingTableNames,
  existingColumnNames,
  onSaveTable,
  onSaveColumn,
}: PropertiesSidebarProps) {
  const [mode, setMode] = useState<"table" | "column">("table");
  const [formData, setFormData] = useState<any>({});
  const [validationErrors, setValidationErrors] = useState<ValidationError[]>(
    []
  );
  const [isSaving, setIsSaving] = useState(false);

  // Initialize form data when sidebar opens
  useEffect(() => {
    if (!isOpen) return;

    if (column) {
      setMode("column");
      setFormData({
        name: column.name,
        raw_type: column.raw_type,
        is_primary: column.is_primary,
        is_nullable: column.is_nullable,
        is_unique: column.is_unique,
        is_indexed: column.is_indexed,
        default_value: column.default_value || "",
        precision: column.precision || 10,
        scale: column.scale || 2,
        enum_values: column.enum_values?.join(", ") || "",
        max_length: column.max_length || undefined,
      });
      setValidationErrors([]);
    } else if (table) {
      setMode("table");
      setFormData({ name: table.name });
      setValidationErrors([]);
    }
  }, [isOpen, table, column]);

  // Validate table name as user types
  useEffect(() => {
    if (mode === "table" && formData.name !== undefined) {
      const result = Validation.validateTableName(
        formData.name,
        existingTableNames,
        tableId
      );
      setValidationErrors(result.errors);
    }
  }, [formData.name, mode, existingTableNames, tableId]);

  // Validate column name and type-specific fields
  useEffect(() => {
    if (mode === "column") {
      const errors: ValidationError[] = [];

      // Validate column name
      const nameResult = Validation.validateColumnName(
        formData.name,
        existingColumnNames,
        column?.name
      );
      errors.push(...nameResult.errors);

      // Validate type-specific fields
      if (formData.raw_type === "decimal") {
        const decimalResult = Validation.validateDecimal(
          formData.precision,
          formData.scale
        );
        errors.push(...decimalResult.errors);
      }

      if (formData.raw_type === "enum") {
        const enumResult = Validation.validateEnumValues(formData.enum_values);
        errors.push(...enumResult.errors);
      }

      setValidationErrors(errors);
    }
  }, [
    formData.name,
    formData.raw_type,
    formData.precision,
    formData.scale,
    formData.enum_values,
    mode,
    existingColumnNames,
    column?.id,
  ]);

  const handleSave = async () => {
    if (validationErrors.length > 0) return;

    setIsSaving(true);

    if (mode === "table" && onSaveTable && tableId) {
      onSaveTable(tableId, formData.name);
    } else if (mode === "column" && onSaveColumn && tableId && column) {
      const updatedColumn: CanvasColumn = {
        ...column,
        name: formData.name,
        raw_type: formData.raw_type,
        is_primary: formData.is_primary,
        is_nullable: formData.is_nullable,
        is_unique: formData.is_unique,
        is_indexed: formData.is_indexed,
        default_value: formData.default_value || undefined,
        precision:
          formData.raw_type === "decimal" ? formData.precision : undefined,
        scale: formData.raw_type === "decimal" ? formData.scale : undefined,
        enum_values:
          formData.raw_type === "enum"
            ? formData.enum_values
                .split(",")
                .map((v: string) => v.trim())
                .filter((v: string) => v)
            : undefined,
        max_length:
          formData.raw_type === "string" ? formData.max_length : undefined,
      };
      onSaveColumn(tableId, column.id, updatedColumn);
    }

    setTimeout(() => {
      setIsSaving(false);
      onClose();
    }, 300);
  };

  if (!isOpen) return null;

  const hasErrors = validationErrors.length > 0;
  const isEditable = mode === "table" ? !!table : !!column;

  return (
    <>
      {/* Overlay */}
      <div
        className="fixed inset-0 bg-black/40 z-30"
        onClick={onClose}
        aria-hidden="true"
      />

      {/* Sidebar Panel */}
      <div className="fixed right-0 top-0 h-full w-80 bg-zinc-900 border-l border-zinc-700 shadow-2xl z-40 flex flex-col animate-in slide-in-from-right-80 duration-200">
        {/* Header */}
        <div className="flex items-center justify-between p-4 border-b border-zinc-700">
          <h2 className="text-lg font-bold text-white">
            {mode === "table" ? "Edit Table" : "Edit Column"}
          </h2>
          <button
            onClick={onClose}
            className="text-zinc-400 hover:text-white transition-colors p-1"
            aria-label="Close sidebar"
          >
            <svg className="w-5 h-5" fill="currentColor" viewBox="0 0 20 20">
              <path
                fillRule="evenodd"
                d="M4.293 4.293a1 1 0 011.414 0L10 8.586l4.293-4.293a1 1 0 111.414 1.414L11.414 10l4.293 4.293a1 1 0 01-1.414 1.414L10 11.414l-4.293 4.293a1 1 0 01-1.414-1.414L8.586 10 4.293 5.707a1 1 0 010-1.414z"
                clipRule="evenodd"
              />
            </svg>
          </button>
        </div>

        {/* Form Content */}
        <div className="flex-1 overflow-y-auto p-4 space-y-4">
          {!isEditable ? (
            <div className="text-center py-8 text-zinc-400">
              <p>Select a table or column to edit</p>
            </div>
          ) : (
            <>
              {/* Name Field */}
              <div>
                <label className="block text-sm font-semibold text-white mb-2">
                  {mode === "table" ? "Table Name" : "Column Name"}
                </label>
                <input
                  type="text"
                  value={formData.name || ""}
                  onChange={(e) =>
                    setFormData({ ...formData, name: e.target.value })
                  }
                  className={`w-full px-3 py-2 rounded-md font-mono text-sm transition-colors ${
                    validationErrors.some((e) => e.field === "name")
                      ? "bg-red-900/20 border border-red-500 text-white placeholder-red-400"
                      : "bg-zinc-800 border border-zinc-700 text-white placeholder-zinc-500 focus:border-cyan-500"
                  } focus:outline-none`}
                  placeholder={
                    mode === "table" ? "users" : "email"
                  }
                />
                {validationErrors
                  .filter((e) => e.field === "name")
                  .map((error) => (
                    <p key={error.field} className="text-xs text-red-400 mt-1">
                      {error.message}
                    </p>
                  ))}
              </div>

              {/* Column-specific fields */}
              {mode === "column" && (
                <>
                  {/* Data Type Selector */}
                  <div>
                    <label className="block text-sm font-semibold text-white mb-2">
                      Data Type
                    </label>
                    <select
                      value={formData.raw_type || "string"}
                      onChange={(e) =>
                        setFormData({
                          ...formData,
                          raw_type: e.target.value as DataType,
                        })
                      }
                      className="w-full px-3 py-2 rounded-md bg-zinc-800 border border-zinc-700 text-white focus:border-cyan-500 focus:outline-none transition-colors"
                    >
                      <option value="string">String</option>
                      <option value="text">Text</option>
                      <option value="smallint">Small Integer</option>
                      <option value="int">Integer</option>
                      <option value="bigint">Big Integer</option>
                      <option value="float">Float</option>
                      <option value="decimal">Decimal</option>
                      <option value="boolean">Boolean</option>
                      <option value="datetime">DateTime</option>
                      <option value="json">JSON</option>
                      <option value="uuid">UUID</option>
                      <option value="enum">Enum</option>
                    </select>
                  </div>

                  {/* Decimal precision/scale (conditional) */}
                  {formData.raw_type === "decimal" && (
                    <div className="space-y-3">
                      <div>
                        <label className="block text-sm font-semibold text-white mb-2">
                          Precision
                        </label>
                        <input
                          type="number"
                          min="1"
                          max="65"
                          value={formData.precision || 10}
                          onChange={(e) =>
                            setFormData({
                              ...formData,
                              precision: parseInt(e.target.value),
                            })
                          }
                          className={`w-full px-3 py-2 rounded-md text-sm transition-colors ${
                            validationErrors.some((e) => e.field === "precision")
                              ? "bg-red-900/20 border border-red-500 text-white"
                              : "bg-zinc-800 border border-zinc-700 text-white focus:border-cyan-500"
                          } focus:outline-none`}
                        />
                        {validationErrors
                          .filter((e) => e.field === "precision")
                          .map((error) => (
                            <p key={error.field} className="text-xs text-red-400 mt-1">
                              {error.message}
                            </p>
                          ))}
                      </div>
                      <div>
                        <label className="block text-sm font-semibold text-white mb-2">
                          Scale
                        </label>
                        <input
                          type="number"
                          min="0"
                          max={formData.precision || 65}
                          value={formData.scale || 2}
                          onChange={(e) =>
                            setFormData({
                              ...formData,
                              scale: parseInt(e.target.value),
                            })
                          }
                          className={`w-full px-3 py-2 rounded-md text-sm transition-colors ${
                            validationErrors.some((e) => e.field === "scale")
                              ? "bg-red-900/20 border border-red-500 text-white"
                              : "bg-zinc-800 border border-zinc-700 text-white focus:border-cyan-500"
                          } focus:outline-none`}
                        />
                        {validationErrors
                          .filter((e) => e.field === "scale")
                          .map((error) => (
                            <p key={error.field} className="text-xs text-red-400 mt-1">
                              {error.message}
                            </p>
                          ))}
                      </div>
                    </div>
                  )}

                  {/* Enum values (conditional) */}
                  {formData.raw_type === "enum" && (
                    <div>
                      <label className="block text-sm font-semibold text-white mb-2">
                        Enum Values (comma-separated)
                      </label>
                      <textarea
                        value={formData.enum_values || ""}
                        onChange={(e) =>
                          setFormData({
                            ...formData,
                            enum_values: e.target.value,
                          })
                        }
                        className={`w-full px-3 py-2 rounded-md text-sm font-mono resize-none transition-colors ${
                          validationErrors.some((e) => e.field === "enum_values")
                            ? "bg-red-900/20 border border-red-500 text-white"
                            : "bg-zinc-800 border border-zinc-700 text-white focus:border-cyan-500"
                        } focus:outline-none`}
                        rows={3}
                        placeholder="active, inactive, pending"
                      />
                      {validationErrors
                        .filter((e) => e.field === "enum_values")
                        .map((error) => (
                          <p key={error.field} className="text-xs text-red-400 mt-1">
                            {error.message}
                          </p>
                        ))}
                    </div>
                  )}

                  {/* String max_length (conditional) */}
                  {formData.raw_type === "string" && (
                    <div>
                      <label className="block text-sm font-semibold text-white mb-2">
                        Max Length (optional)
                      </label>
                      <input
                        type="number"
                        min="1"
                        value={formData.max_length || ""}
                        onChange={(e) =>
                          setFormData({
                            ...formData,
                            max_length: e.target.value
                              ? parseInt(e.target.value)
                              : undefined,
                          })
                        }
                        className="w-full px-3 py-2 rounded-md bg-zinc-800 border border-zinc-700 text-white focus:border-cyan-500 focus:outline-none transition-colors"
                        placeholder="e.g., 255"
                      />
                    </div>
                  )}

                  {/* Default Value */}
                  <div>
                    <label className="block text-sm font-semibold text-white mb-2">
                      Default Value (optional)
                    </label>
                    <input
                      type="text"
                      value={formData.default_value || ""}
                      onChange={(e) =>
                        setFormData({
                          ...formData,
                          default_value: e.target.value,
                        })
                      }
                      className="w-full px-3 py-2 rounded-md bg-zinc-800 border border-zinc-700 text-white placeholder-zinc-500 focus:border-cyan-500 focus:outline-none transition-colors"
                      placeholder="e.g., now()"
                    />
                  </div>

                  {/* Constraint Toggles */}
                  <div className="space-y-2 pt-2 border-t border-zinc-700">
                    <label className="flex items-center gap-3 cursor-pointer group">
                      <input
                        type="checkbox"
                        checked={formData.is_primary || false}
                        onChange={(e) =>
                          setFormData({
                            ...formData,
                            is_primary: e.target.checked,
                          })
                        }
                        className="w-4 h-4 rounded accent-cyan-500"
                      />
                      <span className="text-sm font-medium text-white group-hover:text-cyan-300 transition-colors">
                        Primary Key
                      </span>
                    </label>

                    <label className="flex items-center gap-3 cursor-pointer group">
                      <input
                        type="checkbox"
                        checked={formData.is_nullable || false}
                        onChange={(e) =>
                          setFormData({
                            ...formData,
                            is_nullable: e.target.checked,
                          })
                        }
                        className="w-4 h-4 rounded accent-cyan-500"
                      />
                      <span className="text-sm font-medium text-white group-hover:text-cyan-300 transition-colors">
                        Nullable
                      </span>
                    </label>

                    <label className="flex items-center gap-3 cursor-pointer group">
                      <input
                        type="checkbox"
                        checked={formData.is_unique || false}
                        onChange={(e) =>
                          setFormData({
                            ...formData,
                            is_unique: e.target.checked,
                          })
                        }
                        className="w-4 h-4 rounded accent-cyan-500"
                      />
                      <span className="text-sm font-medium text-white group-hover:text-cyan-300 transition-colors">
                        Unique
                      </span>
                    </label>

                    <label className="flex items-center gap-3 cursor-pointer group">
                      <input
                        type="checkbox"
                        checked={formData.is_indexed || false}
                        onChange={(e) =>
                          setFormData({
                            ...formData,
                            is_indexed: e.target.checked,
                          })
                        }
                        className="w-4 h-4 rounded accent-cyan-500"
                      />
                      <span className="text-sm font-medium text-white group-hover:text-cyan-300 transition-colors">
                        Indexed
                      </span>
                    </label>
                  </div>
                </>
              )}
            </>
          )}
        </div>

        {/* Footer Buttons */}
        {isEditable && (
          <div className="border-t border-zinc-700 p-4 flex gap-2">
            <button
              onClick={onClose}
              className="flex-1 px-4 py-2 rounded-md bg-zinc-800 text-white hover:bg-zinc-700 transition-colors font-semibold"
            >
              Cancel
            </button>
            <button
              onClick={handleSave}
              disabled={hasErrors || isSaving}
              className={`flex-1 px-4 py-2 rounded-md font-semibold transition-colors ${
                hasErrors || isSaving
                  ? "bg-cyan-900/40 text-cyan-400/50 cursor-not-allowed"
                  : "bg-cyan-600 text-white hover:bg-cyan-500"
              }`}
            >
              {isSaving ? "Saving..." : "Save"}
            </button>
          </div>
        )}
      </div>
    </>
  );
}
