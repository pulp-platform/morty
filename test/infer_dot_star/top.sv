
module top #(
  parameter int unsigned TestPara = 4,
  parameter int unsigned TestParaOne = 4,
  parameter int unsigned TestParaTwo = 4,
  parameter int unsigned TestParaThree = 4
) (
  input  logic clk_i,
  input  logic rst_ni,

  input  logic signal_one_i,
  output logic signal_two_o,
  input  logic unused_signal
);

  (* dont_touch *)
  submodule #(
    .SubPara(TestPara)
  ) i_submodule_test (
    .rst_ni(rst_ni),
    .*
  ); 

endmodule

