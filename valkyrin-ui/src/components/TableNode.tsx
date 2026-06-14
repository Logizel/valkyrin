import { Handle, Position } from "@xyflow/react";
import type { CanvasColumn, TableNodeData } from "../types";

interface TableNodeProps {
  id: string;
  data: TableNodeData;
  isConnectable: boolean;
}

/**
 * TableNode: A beautifully designed table node with constraint icons
 * Displays columns with at-a-glance visual indicators for:
 * - Primary Key (🔑)
 * - Unique constraint (U)
 * - Indexed (📇)
 * - Nullable (⊘)
 */
export default function TableNode({
  id,
  data,
  isConnectable,
}: TableNodeProps) {
  const handleAddColumn = () => {
    if (data.onAddColumn) {
      data.onAddColumn(id);
    }
  };

  const handleDeleteTable = () => {
    if (data.onDeleteTable) {
      data.onDeleteTable(id);
    }
  };

  const handleEditTable = () => {
    if (data.onOpenProperties) {
      data.onOpenProperties(id);
    }
  };

  const handleColumnClick = (col: CanvasColumn) => {
    if (data.onOpenProperties) {
      data.onOpenProperties(id, col.id);
    }
  };

  const handleDeleteColumn = (colId: string) => {
    if (data.onDeleteColumn) {
      data.onDeleteColumn(id, colId);
    }
  };

  return (
    <div className="bg-zinc-800 border-2 border-zinc-600 hover:border-cyan-500 rounded-lg w-72 shadow-2xl text-white font-sans group transition-colors duration-200">
      {/* Table Header */}
      <div className="bg-gradient-to-r from-zinc-700 to-zinc-800 p-3 rounded-t-md font-bold text-center border-b border-zinc-600 relative cursor-pointer hover:from-zinc-600 hover:to-zinc-700 transition-colors"
        onClick={handleEditTable}
        title="Click to edit table name"
      >
        <span className="text-cyan-300">{data.label}</span>
        {/* Delete Table Button - Appears on Hover */}
        <button
          onClick={(e) => {
            e.stopPropagation();
            handleDeleteTable();
          }}
          className="absolute right-2 top-2 text-red-400 hover:text-red-300 opacity-0 group-hover:opacity-100 transition-opacity font-bold p-1 hover:bg-red-900/20 rounded"
          title="Delete table"
        >
          ✕
        </button>
      </div>

      {/* Column List */}
      <div className="p-2 space-y-1">
        {data.columns?.map((col) => (
          <ColumnRow
            key={col.id}
            column={col}
            onEdit={() => handleColumnClick(col)}
            onDelete={() => handleDeleteColumn(col.id)}
          />
        ))}

        {/* Add Column Button */}
        <button
          onClick={handleAddColumn}
          className="w-full mt-3 text-xs bg-emerald-600 hover:bg-emerald-500 text-white py-2 rounded-md transition-colors font-semibold"
          title="Add a new column to this table"
        >
          + Add Column
        </button>
      </div>

      {/* Connection Handles */}
      <Handle
        type="target"
        position={Position.Left}
        isConnectable={isConnectable}
      />
      <Handle
        type="source"
        position={Position.Right}
        isConnectable={isConnectable}
      />
    </div>
  );
}

/**
 * ColumnRow: A single column display with constraint badges
 */
function ColumnRow({
  column,
  onEdit,
  onDelete,
}: {
  column: CanvasColumn;
  onEdit: () => void;
  onDelete: () => void;
}) {
  return (
    <div
      className="flex justify-between items-center text-xs bg-zinc-900/50 border border-zinc-700 hover:border-cyan-600/50 p-2 rounded-md group/col cursor-pointer transition-all duration-150 hover:bg-zinc-800/50"
      onClick={onEdit}
      title="Click to edit column"
    >
      {/* Column Name & Type */}
      <div className="flex flex-col gap-1 flex-1 min-w-0">
        <div className="flex items-center gap-2">
          <span className="font-mono text-blue-300 truncate font-semibold">
            {column.name}
          </span>
          <span className="text-zinc-500 text-[11px] flex-shrink-0">
            {column.raw_type}
          </span>
        </div>

        {/* Constraint Badges */}
        <div className="flex gap-1 flex-wrap">
          {column.is_primary && (
            <span
              className="inline-flex items-center gap-0.5 px-1.5 py-0.5 bg-yellow-900/30 border border-yellow-600/50 text-yellow-400 rounded text-[10px] font-semibold"
              title="Primary Key"
            >
              🔑 PK
            </span>
          )}
          {column.is_unique && (
            <span
              className="inline-flex items-center gap-0.5 px-1.5 py-0.5 bg-purple-900/30 border border-purple-600/50 text-purple-300 rounded text-[10px] font-semibold"
              title="Unique constraint"
            >
              ✓ U
            </span>
          )}
          {column.is_indexed && (
            <span
              className="inline-flex items-center gap-0.5 px-1.5 py-0.5 bg-blue-900/30 border border-blue-600/50 text-blue-300 rounded text-[10px] font-semibold"
              title="Indexed"
            >
              📇 IDX
            </span>
          )}
          {column.is_nullable && (
            <span
              className="inline-flex items-center gap-0.5 px-1.5 py-0.5 bg-zinc-700/50 border border-zinc-600/50 text-zinc-400 rounded text-[10px]"
              title="Nullable"
            >
              ⊘
            </span>
          )}
          {column.default_value && (
            <span
              className="inline-flex items-center gap-0.5 px-1.5 py-0.5 bg-green-900/20 border border-green-600/30 text-green-400 rounded text-[10px]"
              title={`Default: ${column.default_value}`}
            >
              ≈
            </span>
          )}
        </div>
      </div>

      {/* Delete Button */}
      <button
        onClick={(e) => {
          e.stopPropagation();
          onDelete();
        }}
        className="ml-2 text-red-400 hover:text-red-300 opacity-0 group-hover/col:opacity-100 transition-opacity font-bold p-1 flex-shrink-0 hover:bg-red-900/20 rounded"
        title="Delete column"
      >
        ✕
      </button>
    </div>
  );
}
