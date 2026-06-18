import { describe, it, expect, vi, beforeEach } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import TableNode from "../TableNode";
import type { CanvasColumn, TableNodeData } from "../../types";
import { ReactFlowWrapper } from "../../test/ReactFlowWrapper";

describe("TableNode", () => {
  const mockColumns: CanvasColumn[] = [
    {
      id: "col-1",
      name: "id",
      raw_type: "uuid",
      is_primary: true,
      is_nullable: false,
      is_unique: false,
      is_indexed: false,
    },
    {
      id: "col-2",
      name: "email",
      raw_type: "string",
      is_primary: false,
      is_nullable: false,
      is_unique: true,
      is_indexed: true,
      default_value: undefined,
    },
    {
      id: "col-3",
      name: "status",
      raw_type: "enum",
      is_primary: false,
      is_nullable: true,
      is_unique: false,
      is_indexed: true,
      enum_values: ["active", "inactive", "pending"],
    },
  ];

  const mockData: TableNodeData = {
    label: "users",
    columns: mockColumns,
    onAddColumn: vi.fn(),
    onDeleteColumn: vi.fn(),
    onDeleteTable: vi.fn(),
    onOpenProperties: vi.fn(),
  };

  const defaultProps = {
    id: "table-1",
    data: mockData,
    isConnectable: true,
  };

  const renderWithWrapper = (props = defaultProps) => {
    return render(
      <ReactFlowWrapper>
        <TableNode {...props} />
      </ReactFlowWrapper>
    );
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders table name", () => {
    renderWithWrapper();
    expect(screen.getByText("users")).toBeInTheDocument();
  });

  it("renders all columns with correct names", () => {
    renderWithWrapper();
    expect(screen.getByText("id")).toBeInTheDocument();
    expect(screen.getByText("email")).toBeInTheDocument();
    expect(screen.getByText("status")).toBeInTheDocument();
  });

  it("shows primary key badge for primary column", () => {
    renderWithWrapper();
    expect(screen.getByText("PK")).toBeInTheDocument();
  });

  it("shows unique badge for unique column", () => {
    renderWithWrapper();
    expect(screen.getByText("✓ U")).toBeInTheDocument();
  });

  it("shows indexed badge for indexed columns", () => {
    renderWithWrapper();
    const idxBadges = screen.getAllByText("IDX");
    expect(idxBadges.length).toBe(2); // email and status columns are indexed
  });

  it("shows nullable badge for nullable column", () => {
    renderWithWrapper();
    expect(screen.getByText("Ø")).toBeInTheDocument();
  });

  it("calls onOpenProperties when table header is clicked", () => {
    renderWithWrapper();
    const tableHeader = screen.getByText("users").closest("div");
    fireEvent.click(tableHeader!);
    expect(defaultProps.data.onOpenProperties).toHaveBeenCalledWith("table-1");
  });

  it("calls onAddColumn when Add Column button is clicked", () => {
    renderWithWrapper();
    const addButton = screen.getByText("+ Add Column");
    fireEvent.click(addButton);
    expect(defaultProps.data.onAddColumn).toHaveBeenCalledWith("table-1");
  });

  it("calls onDeleteTable when delete button is clicked", () => {
    renderWithWrapper();
    // Table delete button has title "Delete table"
    const deleteButton = screen.getByTitle("Delete table");
    fireEvent.click(deleteButton);
    expect(defaultProps.data.onDeleteTable).toHaveBeenCalledWith("table-1");
  });

  it("calls onOpenProperties when column is clicked", () => {
    renderWithWrapper();
    const columnRow = screen.getByText("email").closest("div");
    fireEvent.click(columnRow!);
    expect(defaultProps.data.onOpenProperties).toHaveBeenCalledWith("table-1", "col-2");
  });

  it("calls onDeleteColumn when column delete button is clicked", () => {
    renderWithWrapper();
    // Column delete buttons have title "Delete column"
    const deleteButtons = screen.getAllByTitle("Delete column");
    // Click the second column's delete button (email column)
    fireEvent.click(deleteButtons[1]);
    expect(defaultProps.data.onDeleteColumn).toHaveBeenCalledWith("table-1", "col-2");
  });

  it("renders enum values in column row", () => {
    renderWithWrapper();
    expect(screen.getByText("enum")).toBeInTheDocument();
  });

  it("handles missing optional props gracefully", () => {
    const minimalData: TableNodeData = {
      label: "minimal",
      columns: [],
    };
    const minimalProps = {
      id: "minimal",
      data: minimalData,
      isConnectable: false,
    };
    expect(() => renderWithWrapper(minimalProps)).not.toThrow();
  });
});