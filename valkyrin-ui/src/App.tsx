import { useCallback, useMemo, useEffect } from "react";
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
import "@xyflow/react/dist/style.css";
import TableNode from "./components/TableNode";

const defaultNodes: Node[] = [
  {
    id: "1",
    type: "table",
    position: { x: 100, y: 100 },
    data: {
      label: "Users",
      columns: [
        {
          id: "col_1",
          name: "id",
          raw_type: "uuid",
          is_primary: true,
          is_nullable: false,
        },
      ],
    },
  },
  {
    id: "2",
    type: "table",
    position: { x: 500, y: 100 },
    data: {
      label: "Sessions",
      columns: [
        {
          id: "col_2",
          name: "session_token",
          raw_type: "string",
          is_primary: false,
          is_nullable: false,
        },
      ],
    },
  },
];

export default function App() {
  const [nodes, setNodes, onNodesChange] = useNodesState<Node>([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>([]);

  const nodeTypes = useMemo(() => ({ table: TableNode }), []);

  useEffect(() => {
    async function fetchBlueprint() {
      try {
        const response = await fetch("/api/load");
        const data = await response.json();

        if (data.tables && data.tables.length > 0) {
          const loadedNodes = data.tables.map((t: any, index: number) => ({
            id: t.id,
            type: "table",
            position: t.position || { x: 100 + index * 300, y: 100 },
            data: { label: t.name, columns: t.columns || [] },
          }));
          setNodes(loadedNodes);

          const loadedEdges = (data.relations || []).map((r: any) => ({
            id: r.id,
            source: r.source_table_id,
            target: r.target_table_id,
            type: "default",
          }));
          setEdges(loadedEdges);
        } else {
          setNodes(defaultNodes);
        }
      } catch (e) {
        console.error("Failed to load blueprint:", e);
        setNodes(defaultNodes);
      }
    }
    fetchBlueprint();
  }, [setNodes, setEdges]);

  // --- NEW CRUD METHODS ---
  const handleAddTable = useCallback(() => {
    const tableName = window.prompt("New Table Name (e.g., Products, Orders):");
    if (!tableName) return;

    const newNode: Node = {
      id: crypto.randomUUID(),
      type: "table",
      position: {
        x: window.innerWidth / 2 - 100,
        y: window.innerHeight / 2 - 100,
      },
      data: {
        label: tableName,
        columns: [
          {
            id: crypto.randomUUID(),
            name: "id",
            raw_type: "uuid",
            is_primary: true,
            is_nullable: false,
          },
        ],
      },
    };

    setNodes((nds) => [...nds, newNode]);
  }, [setNodes]);

  const handleDeleteTable = useCallback(
    (nodeId: string) => {
      if (!window.confirm("Delete this entire table?")) return;
      setNodes((nds) => nds.filter((node) => node.id !== nodeId));
      setEdges((eds) =>
        eds.filter((edge) => edge.source !== nodeId && edge.target !== nodeId),
      );
    },
    [setNodes, setEdges],
  );

  const handleAddColumn = useCallback(
    (nodeId: string) => {
      const colName = window.prompt("Column Name (e.g., email, created_at):");
      if (!colName) return;

      const colType = window.prompt(
        "Data Type (string, int, boolean, datetime, uuid):",
        "string",
      );
      if (!colType) return;

      setNodes((nds) =>
        nds.map((node) => {
          if (node.id === nodeId) {
            const newCol = {
              id: crypto.randomUUID(),
              name: colName,
              raw_type: colType,
              is_primary: false,
              is_nullable: false,
            };
            return {
              ...node,
              data: {
                ...node.data,
                columns: [...(node.data.columns as any[]), newCol],
              },
            };
          }
          return node;
        }),
      );
    },
    [setNodes],
  );

  const handleDeleteColumn = useCallback(
    (nodeId: string, colId: string) => {
      setNodes((nds) =>
        nds.map((node) => {
          if (node.id === nodeId) {
            return {
              ...node,
              data: {
                ...node.data,
                columns: (node.data.columns as any[]).filter(
                  (c) => c.id !== colId,
                ),
              },
            };
          }
          return node;
        }),
      );
    },
    [setNodes],
  );

  // Inject all callbacks into node data
  const nodesWithCallbacks = nodes.map((node) => ({
    ...node,
    data: {
      ...node.data,
      onAddColumn: handleAddColumn,
      onDeleteColumn: handleDeleteColumn,
      onDeleteTable: handleDeleteTable,
    },
  }));

  const onConnect = useCallback(
    (params: Connection) => setEdges((eds) => addEdge(params, eds)),
    [setEdges],
  );

  const saveBlueprint = async () => {
    const payload = {
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
        relation_type: "1:N",
      })),
    };

    try {
      const response = await fetch("/api/save", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload),
      });
      if (response.ok) alert("Blueprint saved successfully!");
    } catch (error) {
      console.error("Network error:", error);
    }
  };

  return (
    <div
      style={{ width: "100vw", height: "100vh", backgroundColor: "#0f172a" }}
    >
      <ReactFlow
        nodes={nodesWithCallbacks}
        edges={edges}
        nodeTypes={nodeTypes}
        onNodesChange={onNodesChange}
        onEdgesChange={onEdgesChange}
        onConnect={onConnect}
        fitView
        colorMode="dark"
      >
        <Controls />
        <Background color="#334155" gap={24} />

        {/* Top Left: Add Table */}
        <Panel position="top-left">
          <button
            onClick={handleAddTable}
            className="bg-emerald-600 hover:bg-emerald-500 text-white px-4 py-2 rounded-md font-bold shadow-lg"
          >
            + New Table
          </button>
        </Panel>

        {/* Top Right: Save Blueprint */}
        <Panel position="top-right">
          <button
            onClick={saveBlueprint}
            className="bg-blue-600 hover:bg-blue-500 text-white px-4 py-2 rounded-md font-bold shadow-lg"
          >
            Save Schema to Disk
          </button>
        </Panel>
      </ReactFlow>
    </div>
  );
}
