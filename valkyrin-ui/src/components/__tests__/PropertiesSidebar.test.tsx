import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import PropertiesSidebar from "../PropertiesSidebar";
import type { CanvasTable, CanvasColumn } from "../../types";
import { ReactFlowWrapper } from "../../test/ReactFlowWrapper";

describe("PropertiesSidebar", () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });

  afterEach(() => {
    vi.useRealTimers();
    vi.clearAllMocks();
  });

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
    },
  ];

  const mockTable: CanvasTable = {
    id: "table-1",
    name: "users",
    columns: mockColumns,
    x: 100,
    y: 100,
  };

  const defaultProps = {
    isOpen: true,
    onClose: vi.fn(),
    tableId: "table-1",
    table: mockTable,
    column: undefined,
    existingTableNames: ["users", "posts"],
    existingColumnNames: ["id", "email"],
    onSaveTable: vi.fn(),
    onSaveColumn: vi.fn(),
  };

  const renderWithWrapper = (props = defaultProps) => {
    return render(
      <ReactFlowWrapper>
        <PropertiesSidebar {...props} />
      </ReactFlowWrapper>
    );
  };

  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("renders table properties when table is provided", () => {
    renderWithWrapper();
    expect(screen.getByText("Edit Table")).toBeInTheDocument();
    expect(screen.getByDisplayValue("users")).toBeInTheDocument();
  });

  it("calls onSaveTable when table name changes and save is clicked", () => {
    renderWithWrapper();
    const input = screen.getByDisplayValue("users");
    fireEvent.change(input, { target: { value: "customers" } });
    fireEvent.blur(input);

    vi.runOnlyPendingTimers();

    const saveButton = screen.getByText("Save");
    expect(saveButton).not.toBeDisabled();
    fireEvent.click(saveButton);

    vi.advanceTimersByTime(300);

    expect(defaultProps.onSaveTable).toHaveBeenCalledWith("table-1", "customers");
    expect(defaultProps.onClose).toHaveBeenCalled();
  });

  it("shows validation error for duplicate table name", () => {
    renderWithWrapper();
    const input = screen.getByDisplayValue("users");
    fireEvent.change(input, { target: { value: "posts" } });
    fireEvent.blur(input);

    expect(screen.getByText("A table with this name already exists")).toBeInTheDocument();

    const saveButton = screen.getByText("Save");
    expect(saveButton).toBeDisabled();
  });

  it("renders column properties when column is provided", () => {
    const props = { ...defaultProps, column: mockColumns[1] };
    renderWithWrapper(props);
    expect(screen.getByText("Edit Column")).toBeInTheDocument();
    expect(screen.getByDisplayValue("email")).toBeInTheDocument();
  });

  it("calls onSaveColumn when column properties change and save is clicked", () => {
    const props = { ...defaultProps, column: mockColumns[1] };
    renderWithWrapper(props);

    const nameInput = screen.getByDisplayValue("email");
    fireEvent.change(nameInput, { target: { value: "email_address" } });
    fireEvent.blur(nameInput);

    vi.runOnlyPendingTimers();

    const saveButton = screen.getByText("Save");
    expect(saveButton).not.toBeDisabled();
    fireEvent.click(saveButton);

    vi.advanceTimersByTime(300);

    expect(defaultProps.onSaveColumn).toHaveBeenCalledWith(
      "table-1",
      "col-2",
      expect.objectContaining({ name: "email_address" })
    );
    expect(defaultProps.onClose).toHaveBeenCalled();
  });

  it("shows validation error for duplicate column name", () => {
    const props = { ...defaultProps, column: mockColumns[1] };
    renderWithWrapper(props);

    const nameInput = screen.getByDisplayValue("email");
    fireEvent.change(nameInput, { target: { value: "id" } });
    fireEvent.blur(nameInput);

    expect(screen.getByText("A column with this name already exists in this table")).toBeInTheDocument();

    const saveButton = screen.getByText("Save");
    expect(saveButton).toBeDisabled();
  });

  it("renders decimal precision and scale fields when type is decimal", () => {
    const decimalColumn: CanvasColumn = {
      ...mockColumns[1],
      id: "col-3",
      name: "price",
      raw_type: "decimal",
      precision: 10,
      scale: 2,
    };
    const props = { ...defaultProps, column: decimalColumn };
    renderWithWrapper(props);

    expect(screen.getByText("Precision")).toBeInTheDocument();
    expect(screen.getByDisplayValue("10")).toBeInTheDocument();
    expect(screen.getByText("Scale")).toBeInTheDocument();
    expect(screen.getByDisplayValue("2")).toBeInTheDocument();
  });

  it("renders enum values input when type is enum", () => {
    const enumColumn: CanvasColumn = {
      ...mockColumns[1],
      id: "col-3",
      name: "status",
      raw_type: "enum",
      enum_values: ["active", "inactive"],
    };
    const props = { ...defaultProps, column: enumColumn };
    renderWithWrapper(props);

    expect(screen.getByText("Enum Values (comma-separated)")).toBeInTheDocument();
    expect(screen.getByDisplayValue("active, inactive")).toBeInTheDocument();
  });

  it("shows max length field for string type", () => {
    const stringColumn: CanvasColumn = {
      ...mockColumns[1],
      id: "col-3",
      name: "description",
      raw_type: "string",
      max_length: 255,
    };
    const props = { ...defaultProps, column: stringColumn };
    renderWithWrapper(props);

    expect(screen.getByText("Max Length (optional)")).toBeInTheDocument();
    expect(screen.getByDisplayValue("255")).toBeInTheDocument();
  });

  it("shows constraint toggles for column mode", () => {
    const props = { ...defaultProps, column: mockColumns[1] };
    renderWithWrapper(props);

    expect(screen.getByText("Primary Key")).toBeInTheDocument();
    expect(screen.getByText("Nullable")).toBeInTheDocument();
    expect(screen.getByText("Unique")).toBeInTheDocument();
    expect(screen.getByText("Indexed")).toBeInTheDocument();
  });

  it("returns null when isOpen is false", () => {
    const props = { ...defaultProps, isOpen: false };
    const { container } = renderWithWrapper(props);
    expect(container.firstChild).toBeNull();
  });

  it("shows 'Select a table or column to edit' when neither table nor column provided", () => {
    const props = { ...defaultProps, table: undefined, tableId: undefined };
    renderWithWrapper(props);
    expect(screen.getByText("Select a table or column to edit")).toBeInTheDocument();
  });

  it("closes sidebar when Cancel button is clicked", () => {
    const props = { ...defaultProps, column: mockColumns[1] };
    renderWithWrapper(props);

    const cancelButton = screen.getByText("Cancel");
    fireEvent.click(cancelButton);
    expect(defaultProps.onClose).toHaveBeenCalled();
  });

  it("shows saving state during save", () => {
    const props = { ...defaultProps, column: mockColumns[1] };
    renderWithWrapper(props);

    const saveButton = screen.getByText("Save");
    expect(saveButton).not.toBeDisabled();
    fireEvent.click(saveButton);
    expect(saveButton).toHaveTextContent("Saving...");
    expect(saveButton).toBeDisabled();
  });
});