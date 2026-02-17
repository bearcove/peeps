import type React from "react";

export type Column<T> = {
  key: string;
  label: React.ReactNode;
  sortable?: boolean;
  width?: string;
  render: (row: T) => React.ReactNode;
};

export type TableProps<T> = {
  columns: readonly Column<T>[];
  rows: readonly T[];
  rowKey: (row: T) => string;
  sortKey?: string;
  sortDir?: "asc" | "desc";
  onSort?: (key: string) => void;
  onRowClick?: (row: T) => void;
  selectedRowKey?: string | null;
  "aria-label"?: string;
};

export function Table<T>({
  columns,
  rows,
  rowKey,
  sortKey,
  sortDir,
  onSort,
  onRowClick,
  selectedRowKey,
  "aria-label": ariaLabel,
}: TableProps<T>) {
  const columnsTemplate = columns.map((column) => column.width ?? "1fr").join(" ");
  const gridStyle: React.CSSProperties = {
    gridTemplateColumns: columnsTemplate,
  };

  return (
    <div
      className="ui-table"
      role="grid"
      aria-label={ariaLabel}
    >
      <div className="ui-table-header" role="row" style={gridStyle}>
        {columns.map((column) => {
          const isSorted = column.key === sortKey;
          const isSortable = column.sortable && onSort;
          const sortIndicator = isSorted && sortDir ? (sortDir === "asc" ? "▲" : "▼") : "";
          const headerContent = (
            <>
              <span>{column.label}</span>
              <span className="ui-table-sort-indicator" aria-hidden="true">{sortIndicator}</span>
            </>
          );

          return isSortable ? (
            <button
              type="button"
              key={column.key}
              className="ui-table-header-cell ui-table-sortable"
              onClick={() => onSort(column.key)}
              aria-sort={isSorted ? (sortDir === "asc" ? "ascending" : "descending") : "none"}
            >
              {headerContent}
            </button>
          ) : (
            <div key={column.key} className="ui-table-header-cell">
              {column.label}
            </div>
          );
        })}
      </div>

      {rows.map((row) => {
        const rowId = rowKey(row);
        const isSelected = selectedRowKey === rowId;
        return (
          <div
            key={rowId}
            className={[
              "ui-table-row",
              isSelected && "ui-table-row--selected",
              onRowClick && "ui-table-row--clickable",
            ].filter(Boolean).join(" ")}
            role="row"
            style={gridStyle}
            onClick={() => onRowClick?.(row)}
            onKeyDown={(event) => {
              if (!onRowClick) return;
              if (event.key === "Enter" || event.key === " ") {
                event.preventDefault();
                onRowClick(row);
              }
            }}
            tabIndex={onRowClick ? 0 : undefined}
          >
            {columns.map((column) => (
              <div className="ui-table-cell" key={`${rowId}-${column.key}`}>
                {column.render(row)}
              </div>
            ))}
          </div>
        );
      })}
    </div>
  );
}
