// Copyright 2018 ETH Zurich and University of Bologna.
//
// Copyright and related rights are licensed under the Solderpad Hardware
// License, Version 0.51 (the "License"); you may not use this file except in
// compliance with the License. You may obtain a copy of the License at
// http://solderpad.org/licenses/SHL-0.51. Unless required by applicable law
// or agreed to in writing, software, hardware and materials distributed under
// this License is distributed on an "AS IS" BASIS, WITHOUT WARRANTIES OR
// CONDITIONS OF ANY KIND, either express or implied. See the License for the
// specific language governing permissions and limitations under the License.

`ifndef COMMON_CELLS_REGISTERS_SVH_
`define COMMON_CELLS_REGISTERS_SVH_

`define FF(__q, __d, __reset_value)                  \
  always_ff @(posedge clk_i or negedge rst_ni) begin \
    if (!rst_ni) begin                               \
      __q <= (__reset_value);                        \
    end else begin                                   \
      __q <= (__d);                                  \
    end                                              \
  end

`define IMPORT(__pkg)  \
    import __pkg::*;

`endif
