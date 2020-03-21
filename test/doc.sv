//! Root Comment

/// Type alias for a data word.
typedef logic [31:0] word_t;

/// First-in First-out Queue
///
/// This module implements a FIFO queue.
///
/// # Safety
/// Be careful, this module may harm your sanity.

// This is an irrelevant comment.

module fifo #(
    /// Set this to a random value.
    parameter N = 1000
)(
    /// The main clock input.
    input logic clk_i,
    /// The first reset output.
    output logic rst1_o,
    /// The second reset output.
    output logic rst2_o
);
    //! Here are some additional details.

    /// Here is some internal typedef.
    typedef word_t [3:0] qword_t;

    /// Some internal signals. Very strange.
    word_t data_d, data_q;

    /// Magic nets.
    word_t magic;
endmodule
