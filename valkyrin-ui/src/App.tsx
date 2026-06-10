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

// The default blueprint if no valid file is found
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
            // CRITICAL FIX: Fallback coordinates if loading an old Phase 11 save file!
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
          // FIX: If the file is completely empty, load the defaults
          setNodes(defaultNodes);
        }
      } catch (e) {
        console.error("Failed to load blueprint:", e);
        setNodes(defaultNodes); // Load defaults on network/parse error
      }
    }
    fetchBlueprint();
  }, [setNodes, setEdges]);

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

  const nodesWithCallbacks = nodes.map((node) => ({
    ...node,
    data: { ...node.data, onAddColumn: handleAddColumn },
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
