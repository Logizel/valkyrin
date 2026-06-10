import { Handle, Position } from "@xyflow/react";

export default function TableNode({ id, data, isConnectable }: any) {
  return (
    <div className="bg-gray-800 border-2 border-gray-600 rounded-lg w-64 shadow-xl text-white font-sans group">
      {/* Table Header */}
      <div className="bg-gray-700 p-2 rounded-t-md font-bold text-center border-b border-gray-600 relative">
        {data.label}
        {/* Delete Table Button - Appears on Hover */}
        <button
          onClick={() => data.onDeleteTable(id)}
          className="absolute right-2 top-1.5 text-xs text-red-400 hover:text-red-500 opacity-0 group-hover:opacity-100 transition-opacity font-bold"
          title="Delete Table"
        >
          ✕
        </button>
      </div>

      {/* Column List */}
      <div className="p-2 space-y-1">
        {data.columns?.map((col: any) => (
          <div
            key={col.id}
            className="flex justify-between items-center text-xs bg-gray-900 p-1.5 rounded group/col"
          >
            <div className="flex gap-2">
              <span className="font-mono text-blue-300">
                {col.is_primary && "🔑 "}
                {col.name}
              </span>
              <span className="text-gray-400">{col.raw_type}</span>
            </div>
            {/* Delete Column Button - Appears on Hover */}
            <button
              onClick={() => data.onDeleteColumn(id, col.id)}
              className="text-red-400 hover:text-red-500 opacity-0 group-hover/col:opacity-100 transition-opacity font-bold px-1"
              title="Delete Column"
            >
              ✕
            </button>
          </div>
        ))}

        {/* Add Column Button */}
        <button
          onClick={() => data.onAddColumn(id)}
          className="w-full mt-2 text-xs bg-blue-600 hover:bg-blue-500 py-1.5 rounded transition font-semibold"
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
