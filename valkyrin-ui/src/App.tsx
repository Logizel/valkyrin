import { useCallback, useMemo, useEffect, useRef, useState } from "react";
import {
  ReactFlow,
  Controls,
  Background,
  Panel,
  useNodesState,
  useEdgesState,
  addEdge,
} from "@xyflow/react";
import type { Connection, Edge, Node } from "@xyflow/react";
import { Toaster, toast } from "sonner";
import "@xyflow/react/dist/style.css";
import TableNode from "./components/TableNode";
import PropertiesSidebar from "./components/PropertiesSidebar";
import RelationEdge from "./components/RelationEdge";
import type {
  CanvasPayload,
  CanvasTable,
  CanvasColumn,
  TableNodeData,
  RelationType,
} from "./types";

export default function App() {
  const [nodes, setNodes, onNodesChange] = useNodesState<Node<TableNodeData>>(
    []
  );
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([]);

  // Properties Sidebar State
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [selectedTableId, setSelectedTableId] = useState<string | undefined>();
  const [selectedColumnId, setSelectedColumnId] = useState<string | undefined>();

  // Auto-save debounce
  const saveTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Node types
  const nodeTypes = useMemo(() => ({ table: TableNode }), []);
  const edgeTypes = useMemo(() => ({ relation: RelationEdge }), []);

  // Load blueprint on mount
  useEffect(() => {
    async function fetchBlueprint() {
      try {
        const response = await fetch("/api/load");
        const data: CanvasPayload = await response.json();

        if (data.tables && data.tables.length > 0) {
          const loadedNodes: Node<TableNodeData>[] = data.tables.map(
            (table, index) => ({
              id: table.id,
              type: "table",
              position: table.position || { x: 100 + index * 300, y: 100 },
              data: {
                label: table.name,
                columns: table.columns || [],
                onAddColumn: handleAddColumn,
                onDeleteColumn: handleDeleteColumn,
                onDeleteTable: handleDeleteTable,
                onEditTable: handleEditTable,
                onEditColumn: handleEditColumn,
                onOpenProperties: handleOpenProperties,
              },
            })
          );
          setNodes(loadedNodes);

          const loadedEdges: Edge[] = (data.relations || []).map((r) => ({
            id: r.id,
            source: r.source_table_id,
            target: r.target_table_id,
            type: "relation",
            animated: true,
            data: {
              relationshipType: r.relation_type as RelationType,
              onRelationshipChange: (newType: RelationType) => {
                handleRelationshipChange(r.id, newType);
              },
            },
            style: { stroke: "#06b6d4", strokeWidth: 2 },
          }));
          setEdges(loadedEdges);
        }
      } catch (error) {
        console.error("Failed to load blueprint:", error);
        toast.error("Failed to load blueprint");
      }
    }
    fetchBlueprint();
  }, [setNodes, setEdges]);

  // Auto-save debounced
  const triggerAutoSave = useCallback(() => {
    if (saveTimeoutRef.current) {
      clearTimeout(saveTimeoutRef.current);
    }

    saveTimeoutRef.current = setTimeout(() => {
      saveBlueprint(true); // Pass true for silent auto-save
    }, 2000);
  }, []);

  // Ctrl+S save keybinding
  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if ((e.ctrlKey || e.metaKey) && e.key === "s") {
        e.preventDefault();
        saveBlueprint(false); // Manual save shows toast
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [nodes, edges]);

  // Get current table data
  const getTable = (tableId: string): CanvasTable | undefined => {
    const node = nodes.find((n) => n.id === tableId);
    if (!node) return undefined;
    return {
      id: node.id,
      name: node.data.label,
      columns: node.data.columns,
      position: node.position,
    };
  };

  // Get all existing table names (for validation)
  const existingTableNames = nodes.map((n) => n.data.label);

  // Get all existing column names for a table
  const getExistingColumnNames = (tableId: string): string[] => {
    const node = nodes.find((n) => n.id === tableId);
    return node?.data.columns?.map((c) => c.name) || [];
  };

  // CRUD: Add Table
  const handleAddTable = useCallback(() => {
    setSidebarOpen(true);
    setSelectedTableId("__new__");
    setSelectedColumnId(undefined);
  }, []);

  // CRUD: Delete Table
  const handleDeleteTable = useCallback(
    (nodeId: string) => {
      if (!window.confirm("Delete this entire table and all its columns?"))
        return;

      setNodes((nds) => nds.filter((node) => node.id !== nodeId));
      setEdges((eds) =>
        eds.filter((edge) => edge.source !== nodeId && edge.target !== nodeId)
      );
      triggerAutoSave();
    },
    [setNodes, setEdges, triggerAutoSave]
  );

  // CRUD: Add Column
  const handleAddColumn = useCallback(
    (nodeId: string) => {
      setSidebarOpen(true);
      setSelectedTableId(nodeId);
      setSelectedColumnId("__new__");
    },
    []
  );

  // CRUD: Delete Column
  const handleDeleteColumn = useCallback(
    (nodeId: string, colId: string) => {
      setNodes((nds) =>
        nds.map((node) => {
          if (node.id === nodeId) {
            return {
              ...node,
              data: {
                ...node.data,
                columns: node.data.columns?.filter((c) => c.id !== colId) || [],
              },
            };
          }
          return node;
        })
      );
      triggerAutoSave();
    },
    [setNodes, triggerAutoSave]
  );

  // CRUD: Edit Table
  const handleEditTable = useCallback(
    (nodeId: string, newName: string) => {
      setNodes((nds) =>
        nds.map((node) => {
          if (node.id === nodeId) {
            return {
              ...node,
              data: { ...node.data, label: newName },
            };
          }
          return node;
        })
      );
      triggerAutoSave();
    },
    [setNodes, triggerAutoSave]
  );

  // CRUD: Edit Column
  const handleEditColumn = useCallback(
    (nodeId: string, colId: string, newColumn: CanvasColumn) => {
      setNodes((nds) =>
        nds.map((node) => {
          if (node.id === nodeId) {
            return {
              ...node,
              data: {
                ...node.data,
                columns: node.data.columns?.map((c) =>
                  c.id === colId ? newColumn : c
                ) || [newColumn],
              },
            };
          }
          return node;
        })
      );
      triggerAutoSave();
    },
    [setNodes, triggerAutoSave]
  );

  // CRUD: Open Properties Sidebar
  const handleOpenProperties = useCallback(
    (nodeId: string, colId?: string) => {
      setSelectedTableId(nodeId);
      setSelectedColumnId(colId);
      setSidebarOpen(true);
    },
    []
  );

  // CRUD: Change Relationship Type
  const handleRelationshipChange = useCallback(
    (edgeId: string, newType: RelationType) => {
      setEdges((eds) =>
        eds.map((edge) => {
          if (edge.id === edgeId) {
            return {
              ...edge,
              data: {
                ...edge.data,
                relationshipType: newType,
              },
            };
          }
          return edge;
        })
      );
      triggerAutoSave();
    },
    [setEdges, triggerAutoSave]
  );

  // Save Blueprint to server
  const saveBlueprint = async (isSilent: boolean = false) => {
    const payload: CanvasPayload = {
      tables: nodes.map((n) => ({
        id: n.id,
        name: n.data.label,
        columns: n.data.columns || [],
        position: n.position,
      })),
      relations: edges.map((e) => ({
        id: e.id,
        source_table_id: e.source,
        target_table_id: e.target,
        relation_type: (e.data?.relationshipType || "1:N") as RelationType,
      })),
    };

    try {
      if (!isSilent) {
        toast.loading("Saving blueprint...");
      }

      const response = await fetch("/api/save", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload),
      });

      if (response.ok) {
        if (!isSilent) {
          toast.dismiss();
          toast.success("Blueprint saved successfully");
        }
      } else {
        throw new Error("Server returned error");
      }
    } catch (error) {
      console.error("Save failed:", error);
      toast.error("Error: Could not reach local Valkyrin server");
    }
  };

  // Properties Sidebar callbacks
  const handleSaveTable = useCallback(
    (tableId: string, newName: string) => {
      if (tableId === "__new__") {
        // Add new table
        const newTable: Node<TableNodeData> = {
          id: crypto.randomUUID(),
          type: "table",
          position: {
            x: window.innerWidth / 2 - 100,
            y: window.innerHeight / 2 - 100,
          },
          data: {
            label: newName,
            columns: [
              {
                id: crypto.randomUUID(),
                name: "id",
                raw_type: "uuid",
                is_primary: true,
                is_nullable: false,
                is_unique: false,
                is_indexed: false,
              },
            ],
            onAddColumn: handleAddColumn,
            onDeleteColumn: handleDeleteColumn,
            onDeleteTable: handleDeleteTable,
            onEditTable: handleEditTable,
            onEditColumn: handleEditColumn,
            onOpenProperties: handleOpenProperties,
          },
        };
        setNodes((nds) => [...nds, newTable]);
      } else {
        handleEditTable(tableId, newName);
      }
      triggerAutoSave();
    },
    [handleAddColumn, handleDeleteColumn, handleDeleteTable, handleEditTable, handleEditColumn, handleOpenProperties, setNodes, triggerAutoSave]
  );

  const handleSaveColumn = useCallback(
    (tableId: string, columnId: string, column: CanvasColumn) => {
      if (columnId === "__new__") {
        const newColumn: CanvasColumn = {
          ...column,
          id: crypto.randomUUID(),
        };
        setNodes((nds) =>
          nds.map((node) => {
            if (node.id === tableId) {
              return {
                ...node,
                data: {
                  ...node.data,
                  columns: [...(node.data.columns || []), newColumn],
                },
              };
            }
            return node;
          })
        );
      } else {
        handleEditColumn(tableId, columnId, column);
      }
      triggerAutoSave();
    },
    [setNodes, handleEditColumn, triggerAutoSave]
  );

  // Selected table/column for sidebar
  const selectedTable =
    selectedTableId && selectedTableId !== "__new__"
      ? getTable(selectedTableId)
      : selectedTableId === "__new__"
        ? {
            id: "__new__",
            name: "",
            columns: [],
            position: { x: 0, y: 0 },
          }
        : undefined;

  const selectedColumn =
    selectedTableId && selectedColumnId
      ? selectedColumnId === "__new__"
        ? {
            id: "__new__",
            name: "",
            raw_type: "string" as const,
            is_primary: false,
            is_nullable: false,
            is_unique: false,
            is_indexed: false,
          }
        : getTable(selectedTableId)?.columns.find((c) => c.id === selectedColumnId)
      : undefined;

  const onConnect = useCallback(
    (params: Connection) => {
      const newEdge = addEdge(
        {
          ...params,
          type: "relation",
          animated: true,
          data: {
            relationshipType: "1:N" as RelationType,
            onRelationshipChange: (newType: RelationType) => {
              const edgeId = `${params.source}-${params.target}`;
              handleRelationshipChange(edgeId, newType);
            },
          },
          style: { stroke: "#06b6d4", strokeWidth: 2 },
        },
        edges
      );
      setEdges(newEdge);
      triggerAutoSave();
    },
    [edges, setEdges, triggerAutoSave]
  );

  // Update node callbacks when handlers change
  const nodesWithCallbacks = nodes.map((node) => ({
    ...node,
    data: {
      ...node.data,
      onAddColumn: handleAddColumn,
      onDeleteColumn: handleDeleteColumn,
      onDeleteTable: handleDeleteTable,
      onEditTable: handleEditTable,
      onEditColumn: handleEditColumn,
      onOpenProperties: handleOpenProperties,
    },
  }));

  return (
    <div style={{ width: "100vw", height: "100vh" }} className="bg-zinc-950">
      {/* Toast Notifications */}
      <Toaster position="top-right" theme="dark" />

      {/* React Flow Canvas */}
      <ReactFlow
        nodes={nodesWithCallbacks}
        edges={edges}
        nodeTypes={nodeTypes}
        edgeTypes={edgeTypes}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onConnect={onConnect}
        fitView
        colorMode="dark"
      >
        <Controls />
        <Background color="#27272a" gap={24} />

        {/* Top Left: Add Table Button */}
        <Panel position="top-left">
          <button
            onClick={handleAddTable}
            className="bg-emerald-600 hover:bg-emerald-500 text-white px-4 py-2 rounded-lg font-bold shadow-lg transition-colors"
            title="Add a new table to the canvas"
          >
            + New Table
          </button>
        </Panel>

        {/* Top Right: Save Button + Keyboard Hint */}
        <Panel position="top-right">
          <div className="flex gap-2 items-center">
            <button
              onClick={() => saveBlueprint(false)}
              className="bg-cyan-600 hover:bg-cyan-500 text-white px-4 py-2 rounded-lg font-bold shadow-lg transition-colors"
              title="Save schema to disk (Ctrl+S)"
            >
              Save
            </button>
            <span className="text-xs text-zinc-400 whitespace-nowrap">
              Ctrl+S
            </span>
          </div>
        </Panel>

        {/* Bottom Left: Status Info */}
        <Panel position="bottom-left">
          <div className="text-xs text-zinc-400 space-y-1">
            <p>{nodes.length} table{nodes.length !== 1 ? "s" : ""}</p>
            <p>{edges.length} relation{edges.length !== 1 ? "s" : ""}</p>
            <p className="text-zinc-500 text-[11px] mt-2">
              Auto-saves after 2 seconds
            </p>
          </div>
        </Panel>
      </ReactFlow>

      {/* Properties Sidebar */}
      <PropertiesSidebar
        isOpen={sidebarOpen}
        onClose={() => setSidebarOpen(false)}
        tableId={selectedTableId}
        table={selectedTable}
        column={selectedColumn}
        existingTableNames={existingTableNames}
        existingColumnNames={
          selectedTableId ? getExistingColumnNames(selectedTableId) : []
        }
        onSaveTable={handleSaveTable}
        onSaveColumn={handleSaveColumn}
      />
    </div>
  );
}

