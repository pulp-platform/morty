interface A #(
  parameter B = -1
);

  localparam C = B / 8;

  typedef logic [B-1:0]   by_t;

  b_t        b;

  modport M (
    output b
  );

  modport S (
    input b
  );

endinterface


module D (
    A.M another_interface
);

    A #(.B(2)) a_interface ();

endmodule