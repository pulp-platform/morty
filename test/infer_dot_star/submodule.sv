module submodule #(
  parameter int unsigned SubPara = 4
) (
  input  logic clk_i,
  input  logic rst_ni,

  input  logic signal_one_i,
  output logic signal_two_o
);

  always_comb begin : dummy_comb
    signal_two_o = ~signal_one_i;
  end

endmodule
